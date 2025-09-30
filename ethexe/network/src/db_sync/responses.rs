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
        Config, InnerBehaviour, InnerHashesResponse, InnerProgramIdsResponse, InnerRequest,
        InnerResponse, ResponseId,
    },
    export::PeerId,
};
use ethexe_common::{
    AnnouncesRequest, AnnouncesRequestUntil, AnnouncesResponse,
    db::{
        AnnounceStorageRead, BlockMetaStorageRead, CodesStorageWrite, LatestData,
        LatestDataStorageRead,
    },
};
use ethexe_db::Database;
use libp2p::request_response;
use std::task::{Context, Poll};
use tokio::task::JoinSet;

/// Maximum length of the chain for announces responses to prevent abuse
const MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE: u32 = 10_000;
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
            InnerRequest::Announces(request) => Self::process_announce_request(db, request).into(),
        }
    }

    fn process_announce_request(db: &Database, request: AnnouncesRequest) -> AnnouncesResponse {
        let AnnouncesRequest { head, until } = request;

        // Check the requested chain length first to prevent abuse
        if let AnnouncesRequestUntil::ChainLen(len) = until
            && len > MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE
        {
            log::warn!(
                "Cannot complete request: requested chain len {len} exceeds maximum allowed {MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE}"
            );

            // TODO #4874: use peer score to punish the peer for such requests
            return Default::default();
        }

        let Some(LatestData {
            genesis_announce_hash,
            start_announce_hash,
            ..
        }) = db.latest_data()
        else {
            log::warn!("Cannot complete request: latest data not found in database");

            return Default::default();
        };

        let mut announces = vec![];
        let mut announce_hash = head;
        let mut counter = 0;
        while counter < MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE && announce_hash != start_announce_hash
        {
            let Some(announce) = db.announce(announce_hash) else {
                log::warn!(
                    "Cannot complete request: announce {announce_hash} not found in database"
                );

                return Default::default();
            };

            let parent = announce.parent;
            announces.push(announce);
            counter += 1;

            match until {
                AnnouncesRequestUntil::Tail(tail) if announce_hash == tail => {
                    return AnnouncesResponse { announces };
                }
                AnnouncesRequestUntil::ChainLen(len) if counter == len => {
                    return AnnouncesResponse { announces };
                }
                _ => {}
            }

            announce_hash = parent;
        }

        match until {
            AnnouncesRequestUntil::Tail(tail) => {
                assert_ne!(
                    tail, announce_hash,
                    "If tail match, we must have already found it"
                );

                log::trace!(
                    "Cannot complete request: announce tail {tail} not found in the chain starting from head {head}"
                );

                if announce_hash == start_announce_hash {
                    if start_announce_hash == genesis_announce_hash {
                        // We reached the genesis announce, so it's an invalid request
                        // TODO #4874: use peer score to punish the peer for such requests
                    } else {
                        // We reached the start announce, but it is not the genesis announce,
                        // so this may be a valid request, but we cannot satisfy it
                    }
                } else {
                    // Tail not found within MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE
                    // TODO #4874: use peer score to punish the peer for such requests
                }
            }
            AnnouncesRequestUntil::ChainLen(len) => {
                assert_eq!(
                    announce_hash, start_announce_hash,
                    "If len does not match, only start announce can stop the chain"
                );

                log::trace!(
                    "Cannot complete request: announce chain length {counter} is smaller than requested {len}"
                );

                if start_announce_hash == genesis_announce_hash {
                    // We reached the genesis announce, so it's an invalid request
                    // TODO #4874: use peer score to punish the peer for such requests
                } else {
                    // We reached the start announce, but it is not the genesis announce,
                    // so this may be a valid request, but we cannot satisfy it
                }
            }
        }

        Default::default()
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
