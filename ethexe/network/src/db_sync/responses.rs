// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    db_sync::{
        Config, DbSyncDatabase, InnerBehaviour, InnerHashesResponse, InnerProgramIdsResponse,
        InnerRequest, InnerResponse, ResponseId,
    },
    export::PeerId,
    utils::ParityScaleCodec,
};
use libp2p::request_response;
use parity_scale_codec::{Compact, Encode};
use std::{
    collections::BTreeMap,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

struct OngoingResponse {
    response_id: ResponseId,
    peer_id: PeerId,
    channel: request_response::ResponseChannel<InnerResponse>,
    response: InnerResponse,
}

pub(crate) struct OngoingResponses {
    response_id_counter: u64,
    db: Box<dyn DbSyncDatabase>,
    db_readers: JoinSet<OngoingResponse>,
    max_simultaneous_responses: u32,
}

impl OngoingResponses {
    pub(crate) fn new(db: Box<dyn DbSyncDatabase>, config: &Config) -> Self {
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

    fn response_from_db(request: InnerRequest, db: Box<dyn DbSyncDatabase>) -> InnerResponse {
        const MAX_RESPONSE_SIZE: u64 = ParityScaleCodec::<(), ()>::MAX_RESPONSE_SIZE;

        match request {
            InnerRequest::Hashes(request) => {
                let mut response = BTreeMap::new();
                let mut entries_size = 0;

                for hash in request.0 {
                    let Some(data) = db.read_by_hash(hash) else {
                        continue;
                    };

                    let entry_size = hash.encoded_size() + data.encoded_size();
                    let next_response_size = 1 // InnerResponse discriminant size
                        + Compact((response.len() + 1) as u64).encoded_size()
                        + entries_size
                        + entry_size;

                    if next_response_size > MAX_RESPONSE_SIZE as usize {
                        // don't try to put other hashes data to prevent abusive database reads
                        break;
                    }

                    entries_size += entry_size;
                    response.insert(hash, data);
                }

                InnerHashesResponse(response).into()
            }
            InnerRequest::ProgramIds(request) => {
                let actor_ids = match db.mb_program_states(request.at) {
                    Some(states) => states.into_keys().collect(),
                    None => {
                        log::warn!(
                            "mb_program_states({}) not found; responder returning empty set",
                            request.at,
                        );
                        Default::default()
                    }
                };
                InnerProgramIdsResponse(actor_ids).into()
            }
            InnerRequest::ValidCodes => db.valid_codes().into(),
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

        let db = self.db.clone_boxed();
        self.db_readers.spawn_blocking(move || {
            let response = Self::response_from_db(request, db);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_sync::HashesRequest;
    use ethexe_db::Database;
    use gprimitives::H256;

    #[test]
    fn response_from_db_truncates_hashes_response_at_encoded_limit() {
        const ENTRIES_BEFORE_COMPACT_BOUNDARY: u64 = 0b0011_1111;
        const MAX_RESPONSE_SIZE: usize = ParityScaleCodec::<(), ()>::MAX_RESPONSE_SIZE as usize;

        let db = Database::memory();

        let entries = (0..ENTRIES_BEFORE_COMPACT_BOUNDARY as u8)
            .map(|i| vec![i])
            .collect::<Vec<_>>();
        let entries_size = entries
            .iter()
            .map(|data| H256::zero().encoded_size() + data.encoded_size())
            .sum::<usize>();
        for data in &entries {
            db.cas().write(data);
        }

        let last_entry_size = MAX_RESPONSE_SIZE
            - 1 // `InnerResponse` discriminant
            - Compact(ENTRIES_BEFORE_COMPACT_BOUNDARY + 1).encoded_size()
            - entries_size
            - H256::zero().encoded_size();
        let last_entry = vec![42; last_entry_size];
        let last_entry_hash = db.cas().write(&last_entry);

        let request = entries
            .iter()
            .map(|data| ethexe_db::hash(data))
            .chain(Some(last_entry_hash))
            .collect();
        let response =
            OngoingResponses::response_from_db(HashesRequest(request).into(), Box::new(db));

        let response = response.unwrap_hashes();
        assert_eq!(response.0.len(), ENTRIES_BEFORE_COMPACT_BOUNDARY as usize);
        assert!(InnerResponse::Hashes(response).encoded_size() <= MAX_RESPONSE_SIZE);
    }
}
