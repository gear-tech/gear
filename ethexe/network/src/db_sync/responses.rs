// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    db_sync::{
        Config, InnerAnnouncesRequest, InnerBehaviour, InnerHashesResponse,
        InnerProgramIdsResponse, InnerRequest, InnerResponse, ResponseId,
    },
    export::PeerId,
};
use ethexe_common::db::{
    AnnounceStorageRead, BlockMetaStorageRead, CodesStorageWrite, LatestDataStorageRead,
};
use ethexe_db::Database;
use libp2p::request_response;
use std::task::{Context, Poll};
use tokio::task::JoinSet;

/// Maximum length of the chain for announces responses to prevent abuse
const MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE: u64 = 10_000;
const _: () = assert!(MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE > 0, "cannot be zero");

struct OngoingResponse {
    response_id: ResponseId,
    peer_id: PeerId,
    channel: request_response::ResponseChannel<InnerResponse>,
    response: InnerResponse,
}

pub(crate) struct OngoingResponses {
    response_id_counter: u64,
    db: Database,
    db_readers: JoinSet<OngoingResponse>,
    max_simultaneous_responses: u32,
}

impl OngoingResponses {
    pub(crate) fn new(db: Database, config: &Config) -> Self {
        Self {
            response_id_counter: 0,
            db,
            db_readers: JoinSet::new(),
            max_simultaneous_responses: config.max_simultaneous_responses,
        }
    }

    fn next_response_id(&mut self) -> ResponseId {
        let id = self.response_id_counter;
        self.response_id_counter += 1;
        ResponseId(id)
    }

    fn response_from_db(request: InnerRequest, db: &Database) -> InnerResponse {
        match request {
            InnerRequest::Hashes(request) => InnerHashesResponse(
                request
                    .0
                    .into_iter()
                    .filter_map(|hash| Some((hash, db.read_by_hash(hash)?)))
                    .collect(),
            )
            .into(),
            InnerRequest::ProgramIds(request) => InnerProgramIdsResponse(
                db.block_meta(request.at)
                    .announces
                    .and_then(|a| a.first().copied())
                    .and_then(|announce_hash| {
                        db.announce_program_states(announce_hash)
                            .map(|states| states.into_keys().collect())
                    })
                    .unwrap_or_default(), // FIXME: Option might be more suitable
            )
            .into(),
            InnerRequest::ValidCodes => db.valid_codes().into(),
            InnerRequest::Announces(InnerAnnouncesRequest {
                head,
                max_chain_len,
            }) => {
                if max_chain_len > MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE {
                    log::warn!(
                        "Request for announces with too large max_chain_len: {max_chain_len}, \
                         max is {MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE} instead"
                    );
                    return InnerResponse::Announces(vec![]);
                }

                let Some(start_announce_hash) = db.latest_data().map(|d| d.start_announce_hash)
                else {
                    log::warn!("Cannot complete request: latest data not found in database");
                    return InnerResponse::Announces(vec![]);
                };
                let mut announces = vec![];
                let mut announce_hash = head;
                let mut counter = max_chain_len;
                while counter > 0 && announce_hash != start_announce_hash {
                    let Some(announce) = db.announce(announce_hash) else {
                        log::warn!(
                            "Cannot complete request: announce {announce_hash} not found in database"
                        );
                        announces = vec![];
                        break;
                    };

                    announce_hash = announce.parent;
                    announces.push(announce);
                    counter -= 1;
                }
                InnerResponse::Announces(announces)
            }
        }
    }

    pub(crate) fn handle_response(
        &mut self,
        peer_id: PeerId,
        channel: request_response::ResponseChannel<InnerResponse>,
        request: InnerRequest,
    ) -> Option<ResponseId> {
        if self.db_readers.len() >= self.max_simultaneous_responses as usize {
            return None;
        }

        let response_id = self.next_response_id();

        let db = self.db.clone();
        self.db_readers.spawn_blocking(move || {
            let response = Self::response_from_db(request, &db);
            OngoingResponse {
                response_id,
                peer_id,
                channel,
                response,
            }
        });

        Some(response_id)
    }

    pub(crate) fn poll(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Poll<(PeerId, ResponseId)> {
        if let Poll::Ready(Some(res)) = self.db_readers.poll_join_next(cx) {
            let OngoingResponse {
                response_id,
                peer_id,
                channel,
                response,
            } = res.expect("database panicked");
            let _res = behaviour.send_response(channel, response);
            Poll::Ready((peer_id, response_id))
        } else {
            Poll::Pending
        }
    }
}
