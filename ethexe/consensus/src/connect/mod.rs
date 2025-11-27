// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! # "Connect-Node" Consensus Service
//!
//! Simple "connect-node" consensus service implementation.

use crate::{
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService,
    announces::{self, AnnounceStatus, DBAnnouncesExt},
    utils::{self, AnnouncesRequestState},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::OnChainStorageRO,
    injected::SignedInjectedTransaction,
    network::{AnnouncesRequest, CheckedAnnouncesResponse},
};
use ethexe_db::Database;
use ethexe_network::db_sync::Handle;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use lru::LruCache;
use std::{
    collections::VecDeque,
    mem,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// Maximum number of pending announces to store
const MAX_PENDING_ANNOUNCES: NonZeroUsize = NonZeroUsize::new(10).unwrap();

/// State transition flow:
///
/// ```text
/// WaitingForBlock (waiting for new chain head)
///   └─ receive_new_chain_head ─► WaitingForSyncedBlock
///
/// WaitingForSyncedBlock (waiting block is synced)
///   └─ receive_synced_block ─► WaitingForPreparedBlock
///
/// WaitingForPreparedBlock (waiting block is prepared)
///   ├─ if missing announces ─► WaitingForMissingAnnounces
///   └─ if no missing ─► process_after_propagation
///
/// WaitingForMissingAnnounces (waiting for requested missing announces from network)
///   └─ receive_announces_response ─► process_after_propagation
///
/// process_after_propagation (propagation done )
///   ├─ announce from producer already received ─► emit ComputeAnnounce ─► WaitingForBlock
///   └─ no already received announce ─► WaitingForAnnounce
///
/// WaitingForAnnounce (waiting for announce from producer)
///   ├─ expected and accepted ─► emit ComputeAnnounce and AcceptAnnounce ─► WaitingForBlock
///   └─ unexpected ─► cached in pending_announces
/// ```
#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
enum State {
    WaitingForBlock,
    WaitingForSyncedBlock {
        block: SimpleBlockData,
    },
    WaitingForPreparedBlock {
        block: SimpleBlockData,
        producer: Address,
    },
    WaitingForAnnounce {
        block: SimpleBlockData,
        producer: Address,
    },
    WaitingForMissingAnnounces {
        block: SimpleBlockData,
        producer: Address,
        chain: VecDeque<SimpleBlockData>,
        waiting_request: AnnouncesRequest,
        announces_fetch: Option<AnnouncesRequestState>,
    },
}

/// Consensus service which tracks the on-chain and ethexe events
/// in order to keep the program states actual in local database.
#[derive(derive_more::Debug)]
pub struct ConnectService {
    db: Database,
    slot_duration: Duration,
    commitment_delay_limit: u32,

    state: State,
    pending_announces: LruCache<(Address, H256), Announce>,
    output: VecDeque<ConsensusEvent>,

    #[debug(skip)]
    db_sync_handle: Option<Handle>,
}

impl ConnectService {
    /// Creates a new instance of `ConnectService`.
    ///
    /// # Parameters
    /// - `db`: Database instance.
    /// - `slot_duration`: Duration of each slot in the consensus protocol.
    /// - `commitment_delay_limit`: Maximum allowed delay for announce to be committed.
    /// - `db_sync_handle`: Optional network handle used for db-sync requests; when `None`,
    ///   announces are not fetched from peers.
    pub fn new(
        db: Database,
        slot_duration: Duration,
        commitment_delay_limit: u32,
        db_sync_handle: Option<Handle>,
    ) -> Self {
        Self {
            db,
            slot_duration,
            commitment_delay_limit,
            state: State::WaitingForBlock,
            pending_announces: LruCache::new(MAX_PENDING_ANNOUNCES),
            output: VecDeque::new(),
            db_sync_handle,
        }
    }

    fn process_after_propagation(&mut self, block: SimpleBlockData, producer: Address) {
        if let Some(announce) = self.pending_announces.pop(&(producer, block.hash)) {
            self.output
                .push_back(ConsensusEvent::ComputeAnnounce(announce));
            self.state = State::WaitingForBlock;
        } else {
            self.state = State::WaitingForAnnounce { block, producer };
        }
    }

    fn request_announces(&mut self, request: AnnouncesRequest) {
        let Some(handle) = self.db_sync_handle.clone() else {
            tracing::debug!("Skipping announces request: network handle is not available");
            return;
        };

        match &mut self.state {
            State::WaitingForMissingAnnounces {
                announces_fetch, ..
            } => {
                *announces_fetch = Some(AnnouncesRequestState::new(&handle, request));
            }
            state => panic!("Announces request in unexpected state: {state:?}"),
        }
    }

    fn on_announces_response(&mut self, response: CheckedAnnouncesResponse) -> Result<()> {
        let State::WaitingForMissingAnnounces {
            block,
            producer,
            chain,
            waiting_request,
            ..
        } = &mut self.state
        else {
            return Ok(());
        };

        let block = block.clone();
        let producer = *producer;

        let (request, announces) = response.into_parts();

        if waiting_request != &request {
            return Ok(());
        }

        announces::propagate_announces(
            &self.db,
            mem::take(chain),
            self.commitment_delay_limit,
            announces.into_iter().map(|a| (a.to_hash(), a)).collect(),
        )?;

        self.process_after_propagation(block, producer);

        Ok(())
    }
}

impl ConsensusService for ConnectService {
    fn role(&self) -> String {
        "Connect".to_string()
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.state = State::WaitingForSyncedBlock { block };
        Ok(())
    }

    fn receive_synced_block(&mut self, block_hash: H256) -> Result<()> {
        if let State::WaitingForSyncedBlock { block } = &self.state
            && block.hash == block_hash
        {
            let timelines = self
                .db
                .protocol_timelines()
                .ok_or_else(|| anyhow!("protocol timelines not found in database"))?;
            let block_era = timelines.era_from_ts(block.header.timestamp);
            let validators = self.db.validators(block_era).ok_or(anyhow!(
                "validators not found for synced block({block_hash})"
            ))?;
            let producer = utils::block_producer_for(
                &validators,
                block.header.timestamp,
                self.slot_duration.as_secs(),
            );

            self.state = State::WaitingForPreparedBlock {
                block: block.clone(),
                producer,
            };
        }
        Ok(())
    }

    fn receive_prepared_block(&mut self, prepared_block_hash: H256) -> Result<()> {
        let State::WaitingForPreparedBlock { block, producer } = &self.state else {
            return Ok(());
        };

        if block.hash != prepared_block_hash {
            return Ok(());
        }

        let block = block.clone();
        let producer = *producer;

        let chain = self.db.collect_blocks_without_announces(block.hash)?;

        if let Some(last_with_announces_block_hash) = chain.front().map(|b| b.header.parent_hash)
            && let Some(request) = announces::check_for_missing_announces(
                &self.db,
                block.hash,
                last_with_announces_block_hash,
                self.commitment_delay_limit,
            )?
        {
            tracing::debug!(
                block = %block.hash,
                request = ?request,
                "Requesting missing announces",
            );

            self.state = State::WaitingForMissingAnnounces {
                block: block.clone(),
                producer,
                chain,
                waiting_request: request,
                announces_fetch: None,
            };

            self.output
                .push_back(ConsensusEvent::RequestAnnounces(request));
        } else {
            tracing::debug!(
                block = %block.hash,
                "No missing announces detected",
            );

            announces::propagate_announces(
                &self.db,
                chain,
                self.commitment_delay_limit,
                Default::default(),
            )?;

            self.process_after_propagation(block, producer);
        }

        Ok(())
    }

    fn receive_computed_announce(&mut self, _announce: HashOf<Announce>) -> Result<()> {
        Ok(())
    }

    fn receive_announce(&mut self, announce: VerifiedAnnounce) -> Result<()> {
        let (announce, sender) = announce.clone().into_parts();
        let sender = sender.to_address();

        if let State::WaitingForAnnounce { block, producer } = &self.state
            && sender == *producer
            && announce.block_hash == block.hash
        {
            match announces::accept_announce(&self.db, announce.clone())? {
                AnnounceStatus::Rejected { announce, reason } => {
                    tracing::warn!(
                        announce = %announce.to_hash(),
                        reason = %reason,
                        producer = %producer,
                        "Announce rejected",
                    );

                    self.output
                        .push_back(ConsensusEvent::AnnounceRejected(announce.to_hash()));
                }
                AnnounceStatus::Accepted(announce_hash) => {
                    self.output
                        .push_back(ConsensusEvent::AnnounceAccepted(announce_hash));
                    self.output
                        .push_back(ConsensusEvent::ComputeAnnounce(announce));

                    self.state = State::WaitingForBlock;
                }
            }
        } else {
            tracing::warn!("Receive unexpected {announce:?}, save to pending announces");
            self.pending_announces
                .push((sender, announce.block_hash), announce);
        }

        Ok(())
    }

    fn receive_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()> {
        // In "connect-node" we do not process injected transactions.
        tracing::trace!("Received injected transaction: {tx:?}. Ignoring it.");
        Ok(())
    }

    fn receive_validation_request(&mut self, _batch: VerifiedValidationRequest) -> Result<()> {
        Ok(())
    }

    fn receive_validation_reply(&mut self, _reply: BatchCommitmentValidationReply) -> Result<()> {
        Ok(())
    }

    fn receive_announces_response(&mut self, _response: CheckedAnnouncesResponse) -> Result<()> {
        Ok(())
    }
}

impl Stream for ConnectService {
    type Item = Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(handle) = self.db_sync_handle.clone()
                && let State::WaitingForMissingAnnounces {
                    announces_fetch, ..
                } = &mut self.state
                && let Some(mut fetch) = announces_fetch.take()
            {
                match fetch.poll(&handle, cx) {
                    Poll::Ready(response) => {
                        if let Err(err) = self.on_announces_response(response) {
                            return Poll::Ready(Some(Err(err)));
                        }

                        continue;
                    }
                    Poll::Pending => {
                        *announces_fetch = Some(fetch);
                    }
                }
            }

            if let Some(event) = self.output.pop_front() {
                match event {
                    ConsensusEvent::RequestAnnounces(request) => {
                        self.request_announces(request);
                        continue;
                    }
                    _ => return Poll::Ready(Some(Ok(event))),
                }
            }

            return Poll::Pending;
        }
    }
}

impl FusedStream for ConnectService {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Announce, HashOf,
        db::{BlockMetaStorageRO, BlockMetaStorageRW},
        mock::{BlockChain, DBMockExt, Mock},
        network::{AnnouncesRequestUntil, AnnouncesResponse},
    };
    use ethexe_db::Database;
    use ethexe_network::db_sync::{Request, Response, test_utils::HandleStub};
    use futures::{FutureExt, future::poll_fn};
    use std::{pin::Pin, time::Duration};
    use tokio::time::timeout;

    fn collect_announces(
        chain: &BlockChain,
        head: HashOf<Announce>,
        until: &AnnouncesRequestUntil,
    ) -> Vec<Announce> {
        let mut announces = Vec::new();
        match until {
            AnnouncesRequestUntil::Tail(tail) => {
                let mut current = head;
                loop {
                    let announce = chain
                        .announces
                        .get(&current)
                        .expect("announce not found")
                        .announce
                        .clone();
                    let parent = announce.parent;
                    if current == *tail {
                        break;
                    }
                    announces.push(announce);
                    current = parent;
                }
            }
            AnnouncesRequestUntil::ChainLen(len) => {
                let mut current = head;
                for _ in 0..len.get() {
                    let announce = chain
                        .announces
                        .get(&current)
                        .expect("announce not found")
                        .announce
                        .clone();
                    announces.push(announce.clone());
                    current = announce.parent;
                }
            }
        }
        announces.reverse();
        announces
    }

    #[tokio::test]
    async fn applies_announces_response() {
        let head_index = 5usize;
        let remote_chain = BlockChain::mock(head_index as u32);
        let head_hash = remote_chain.blocks.back().unwrap().hash;
        let remote_db = Database::memory();
        let remote_chain = remote_chain.setup(&remote_db);

        let local_db = remote_db.clone();
        let missing_hashes = [
            remote_chain.blocks.get(head_index - 2).unwrap().hash,
            remote_chain.blocks.get(head_index - 1).unwrap().hash,
            head_hash,
        ];
        for hash in missing_hashes.iter() {
            local_db.mutate_block_meta(*hash, |meta| meta.announces = None);
        }
        let missing_head = remote_chain.block_top_announce_hash(head_index - 2);
        local_db.mutate_block_meta(head_hash, |meta| {
            meta.last_committed_announce = Some(missing_head);
        });
        let chain = local_db
            .collect_blocks_without_announces(head_hash)
            .expect("missing chain");
        let last_with_announces = chain.front().expect("non-empty chain").header.parent_hash;
        let waiting_request =
            announces::check_for_missing_announces(&local_db, head_hash, last_with_announces, 3)
                .expect("request check failed")
                .expect("request expected");

        let mut connect = ConnectService::new(local_db.clone(), Duration::from_secs(1), 3, None);

        connect.state = State::WaitingForMissingAnnounces {
            block: local_db.simple_block_data(head_hash),
            producer: Address::default(),
            chain,
            waiting_request: waiting_request.clone(),
            announces_fetch: None,
        };

        let announces =
            collect_announces(&remote_chain, waiting_request.head, &waiting_request.until);
        let response = AnnouncesResponse { announces }
            .try_into_checked(waiting_request)
            .expect("valid response");
        connect.on_announces_response(response).unwrap();

        assert!(connect.output.is_empty(), "no immediate events expected");
        match &connect.state {
            State::WaitingForAnnounce { block, .. } => {
                assert_eq!(block.hash, head_hash);
            }
            other => panic!("unexpected state after response: {other:?}"),
        }
        let announces = local_db
            .block_meta(head_hash)
            .announces
            .expect("announces must be propagated");
        assert!(!announces.is_empty(), "expected announces to be stored");
    }

    #[tokio::test]
    async fn fetches_missing_announces_via_handle_stub() {
        let head_index = 5usize;
        let remote_chain = BlockChain::mock(head_index as u32);
        let head_hash = remote_chain.blocks.back().unwrap().hash;
        let remote_db = Database::memory();
        let remote_chain = remote_chain.setup(&remote_db);

        let local_db = remote_db.clone();
        let missing_hashes = [
            remote_chain.blocks.get(head_index - 2).unwrap().hash,
            remote_chain.blocks.get(head_index - 1).unwrap().hash,
            head_hash,
        ];
        for hash in missing_hashes.iter() {
            local_db.mutate_block_meta(*hash, |meta| meta.announces = None);
        }
        let last_known = remote_chain.block_top_announce_hash(head_index - 2);
        local_db.mutate_block_meta(head_hash, |meta| {
            meta.last_committed_announce = Some(last_known);
        });

        let mut handle_stub = HandleStub::new();
        let mut connect = ConnectService::new(
            local_db.clone(),
            Duration::from_secs(1),
            3,
            Some(handle_stub.handle()),
        );

        let chain = local_db
            .collect_blocks_without_announces(head_hash)
            .expect("missing chain");
        let last_with_announces = chain.front().expect("non-empty chain").header.parent_hash;
        let expected_request =
            announces::check_for_missing_announces(&local_db, head_hash, last_with_announces, 3)
                .expect("request check failed")
                .expect("request expected");

        let head_block = local_db.simple_block_data(head_hash);
        connect.receive_new_chain_head(head_block.clone()).unwrap();
        connect.receive_synced_block(head_block.hash).unwrap();
        connect.receive_prepared_block(head_block.hash).unwrap();

        // Drive the service once to flush the request into the db-sync handle.
        poll_fn(|cx| Pin::new(&mut connect).poll_next(cx)).now_or_never();

        let (_, inner_request, responder) =
            timeout(Duration::from_secs(1), handle_stub.recv_request())
                .await
                .expect("timeout waiting for stub request");
        let Request::Announces(stub_request) = inner_request else {
            panic!("unexpected request: {inner_request:?}");
        };
        assert_eq!(
            stub_request, expected_request,
            "request forwarded to db-sync"
        );

        let request = stub_request.clone();
        let announces = collect_announces(&remote_chain, request.head, &request.until);
        let response = AnnouncesResponse { announces }
            .try_into_checked(request)
            .expect("valid response");
        responder
            .send(Ok(Response::Announces(response)))
            .expect("send response");

        // Drive the service once more to process the response future.
        timeout(
            Duration::from_secs(1),
            poll_fn(|cx| {
                let _ = Pin::new(&mut connect).poll_next(cx);
                if matches!(connect.state, State::WaitingForAnnounce { .. }) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }),
        )
        .await
        .expect("timeout processing response");

        match &connect.state {
            State::WaitingForAnnounce { block, .. } => {
                assert_eq!(block.hash, head_hash);
            }
            state => panic!("unexpected state after response: {state:?}"),
        }

        let announces = local_db
            .block_meta(head_hash)
            .announces
            .expect("announces must be propagated");
        assert!(!announces.is_empty(), "expected announces to be stored");
    }
}
