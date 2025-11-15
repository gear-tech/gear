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
        Config, DbSyncDatabase, InnerBehaviour, InnerHashesResponse, InnerProgramIdsResponse,
        InnerRequest, InnerResponse, ResponseId,
    },
    export::PeerId,
};
use ethexe_common::{
    Announce, HashOf,
    db::{AnnounceStorageRO, BlockMetaStorageRO, HashStorageRO, LatestData, LatestDataStorageRO},
    network::{AnnouncesRequest, AnnouncesRequestUntil, AnnouncesResponse},
};
use libp2p::request_response;
use std::{
    collections::VecDeque,
    num::NonZeroU32,
    task::{Context, Poll},
};
use thiserror::Error;
use tokio::task::JoinSet;

/// Maximum length of the chain for announces responses to prevent abuse
const MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE: NonZeroU32 = NonZeroU32::new(1000).unwrap();

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
            InnerRequest::Announces(request) => {
                match Self::process_announce_request(&db, request) {
                    Ok(response) => response.into(),
                    Err(e) => {
                        log::warn!("cannot complete request: {e}");
                        InnerResponse::Announces(Default::default())
                    }
                }
            }
        }
    }

    fn process_announce_request<DB: AnnounceStorageRO + LatestDataStorageRO>(
        db: &DB,
        request: AnnouncesRequest,
    ) -> Result<AnnouncesResponse, ProcessAnnounceError> {
        let AnnouncesRequest { head, until } = request;

        // Check the requested chain length first to prevent abuse
        if let AnnouncesRequestUntil::ChainLen(len) = until
            && len > MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE
        {
            // TODO #4874: use peer score to punish the peer for such requests
            return Err(ProcessAnnounceError::ChainLenExceedsMax { requested: len });
        }

        let Some(LatestData {
            genesis_announce_hash,
            start_announce_hash,
            ..
        }) = db.latest_data()
        else {
            return Err(ProcessAnnounceError::LatestDataMissing);
        };

        let mut announces = VecDeque::new();
        let mut announce_hash = head;
        for _ in 0..MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE.get() {
            let Some(announce) = db.announce(announce_hash) else {
                return Err(ProcessAnnounceError::AnnounceMissing {
                    hash: announce_hash,
                });
            };

            let parent = announce.parent;
            announces.push_front(announce);

            match until {
                AnnouncesRequestUntil::Tail(tail) if announce_hash == tail => {
                    return Ok(AnnouncesResponse {
                        announces: announces.into(),
                    });
                }
                AnnouncesRequestUntil::ChainLen(len) if announces.len() == len.get() as usize => {
                    return Ok(AnnouncesResponse {
                        announces: announces.into(),
                    });
                }
                _ => {}
            }

            if announce_hash == start_announce_hash {
                if start_announce_hash == genesis_announce_hash {
                    // Reaching genesis - request is invalid and should be punished.
                    // TODO #4874: use peer score to punish the peer for such requests
                    return Err(ProcessAnnounceError::ReachedGenesis {
                        genesis: genesis_announce_hash,
                    });
                } else {
                    // Reaching start announce - request can be valid, we just can't go further
                    return Err(ProcessAnnounceError::ReachedStart {
                        start: start_announce_hash,
                    });
                }
            }

            announce_hash = parent;
        }

        // TODO #4874: use peer score to punish the peer for such requests
        Err(ProcessAnnounceError::ReachedMaxChainLength)
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

#[derive(Debug, Error, PartialEq, Eq)]
enum ProcessAnnounceError {
    #[error(
        "requested chain length {requested} exceeds maximum allowed {MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE}"
    )]
    ChainLenExceedsMax { requested: NonZeroU32 },
    #[error("latest data not found in database")]
    LatestDataMissing,
    #[error("announce {hash} not found in database")]
    AnnounceMissing { hash: HashOf<Announce> },
    #[error("reached genesis announce {genesis}")]
    ReachedGenesis { genesis: HashOf<Announce> },
    #[error("reached start announce {start}")]
    ReachedStart { start: HashOf<Announce> },
    #[error("reached maximum chain length {MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE}")]
    ReachedMaxChainLength,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Announce, HashOf, SimpleBlockData,
        db::{AnnounceStorageRW, LatestDataStorageRW},
    };
    use ethexe_db::Database;
    use gprimitives::H256;
    use std::num::NonZeroU32;

    fn make_announce(block: u64, parent: HashOf<Announce>) -> Announce {
        Announce::base(H256::from_low_u64_be(block), parent)
    }

    fn set_latest_data(db: &Database, genesis: HashOf<Announce>, start: HashOf<Announce>) {
        db.set_latest_data(LatestData {
            synced_block: SimpleBlockData {
                hash: H256::zero(),
                header: Default::default(),
            },
            prepared_block_hash: H256::zero(),
            computed_announce_hash: HashOf::zero(),
            genesis_block_hash: H256::zero(),
            genesis_announce_hash: genesis,
            start_block_hash: H256::zero(),
            start_announce_hash: start,
        });
    }

    #[test]
    fn fails_chain_len_exceeding_max() {
        let db = Database::memory();
        set_latest_data(&db, HashOf::zero(), HashOf::zero());

        let len = MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE.checked_add(1).unwrap();
        let request = AnnouncesRequest {
            head: HashOf::zero(),
            until: AnnouncesRequestUntil::ChainLen(len),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(
            err,
            ProcessAnnounceError::ChainLenExceedsMax { requested: len }
        );
    }

    #[test]
    fn fails_latest_data_missing() {
        let db = Database::memory();
        let request = AnnouncesRequest {
            head: HashOf::zero(),
            until: AnnouncesRequestUntil::Tail(HashOf::zero()),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(err, ProcessAnnounceError::LatestDataMissing);
    }

    #[test]
    fn fails_announce_missing() {
        let head = HashOf::random();
        let db = Database::memory();
        set_latest_data(&db, HashOf::zero(), HashOf::zero());

        let request = AnnouncesRequest {
            head,
            until: AnnouncesRequestUntil::Tail(HashOf::zero()),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(err, ProcessAnnounceError::AnnounceMissing { hash: head });
    }

    #[test]
    fn fails_when_reaching_genesis() {
        let db = Database::memory();

        let genesis_announce = make_announce(10, HashOf::random());
        let genesis = db.set_announce(genesis_announce);
        let middle = make_announce(11, genesis);
        let middle_hash = db.set_announce(middle.clone());
        let head = make_announce(12, middle_hash);
        let head_hash = db.set_announce(head.clone());

        set_latest_data(&db, genesis, genesis);

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(HashOf::random()),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(err, ProcessAnnounceError::ReachedGenesis { genesis });
    }

    #[test]
    fn fails_reaching_start_non_genesis() {
        let db = Database::memory();
        let start_announce = make_announce(10, HashOf::random());
        let start = db.set_announce(start_announce);
        let genesis = HashOf::random();

        set_latest_data(&db, genesis, start);

        let head = make_announce(11, start);
        let head_hash = db.set_announce(head);

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(HashOf::random()),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(err, ProcessAnnounceError::ReachedStart { start });
    }

    #[test]
    fn fails_reaching_max_chain_length() {
        let db = Database::memory();

        let mut parent = HashOf::random();
        let mut head_hash = parent;
        let mut chain_hashes = Vec::new();

        for i in 0..MAX_CHAIN_LEN_FOR_ANNOUNCES_RESPONSE.get() {
            let announce = make_announce(10_000 + i as u64, parent);
            let hash = db.set_announce(announce);
            chain_hashes.push(hash);
            parent = hash;
            head_hash = hash;
        }

        let start = HashOf::random();
        let genesis = HashOf::random();
        let tail = HashOf::random();

        assert!(!chain_hashes.contains(&start));
        assert!(!chain_hashes.contains(&genesis));
        assert!(!chain_hashes.contains(&tail));

        set_latest_data(&db, genesis, start);

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(tail),
        };

        let err = OngoingResponses::process_announce_request(&db, request).unwrap_err();
        assert_eq!(err, ProcessAnnounceError::ReachedMaxChainLength);
    }

    #[test]
    fn returns_announces_until_tail() {
        let db = Database::memory();

        let tail = make_announce(10, HashOf::random());
        let tail_hash = db.set_announce(tail.clone());
        let head = make_announce(11, tail_hash);
        let head_hash = db.set_announce(head.clone());

        let genesis = HashOf::random();
        let start = HashOf::random();
        set_latest_data(&db, genesis, start);

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(tail_hash),
        };

        let response = OngoingResponses::process_announce_request(&db, request).unwrap();
        assert_eq!(response.announces, vec![tail, head]);
        response.try_into_checked(request).unwrap();
    }

    #[test]
    fn returns_announces_until_chain_len() {
        let db = Database::memory();

        let tail = make_announce(10, HashOf::random());
        let tail_hash = db.set_announce(tail.clone());
        let middle = make_announce(11, tail_hash);
        let middle_hash = db.set_announce(middle.clone());
        let head = make_announce(12, middle_hash);
        let head_hash = db.set_announce(head.clone());

        let genesis = HashOf::random();
        let start = HashOf::random();
        set_latest_data(&db, genesis, start);

        let length = NonZeroU32::new(2).unwrap();
        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::ChainLen(length),
        };

        let response = OngoingResponses::process_announce_request(&db, request).unwrap();
        assert_eq!(response.announces, vec![middle, head]);
        response.try_into_checked(request).unwrap();
    }
}
