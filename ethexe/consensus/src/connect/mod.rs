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
    utils,
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::OnChainStorageRO,
    injected::SignedInjectedTransaction,
    network::{AnnouncesRequest, AnnouncesResponse},
};
use ethexe_db::Database;
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
}

impl ConnectService {
    /// Creates a new instance of `ConnectService`.
    ///
    /// # Parameters
    /// - `db`: Database instance.
    /// - `slot_duration`: Duration of each slot in the consensus protocol.
    /// - `commitment_delay_limit`: Maximum allowed delay for announce to be committed.
    pub fn new(db: Database, slot_duration: Duration, commitment_delay_limit: u32) -> Self {
        Self {
            db,
            slot_duration,
            commitment_delay_limit,
            state: State::WaitingForBlock,
            pending_announces: LruCache::new(MAX_PENDING_ANNOUNCES),
            output: VecDeque::new(),
        }
    }

    fn process_after_propagation(
        &mut self,
        block: SimpleBlockData,
        producer: Address,
    ) -> Result<()> {
        if let Some(announce) = self.pending_announces.pop(&(producer, block.hash)) {
            self.process_announce_from_producer(announce, producer)?;
            self.state = State::WaitingForBlock;
        } else {
            self.state = State::WaitingForAnnounce { block, producer };
        }

        Ok(())
    }

    fn process_announce_from_producer(
        &mut self,
        announce: Announce,
        producer: Address,
    ) -> Result<()> {
        match announces::accept_announce(&self.db, announce.clone())? {
            AnnounceStatus::Rejected { announce, reason } => {
                tracing::warn!(
                    announce = %announce.to_hash(),
                    producer = %producer,
                    "Announce rejected: {reason}",
                );

                self.output
                    .push_back(ConsensusEvent::AnnounceRejected(announce.to_hash()));
            }
            AnnounceStatus::Accepted(announce_hash) => {
                self.output
                    .push_back(ConsensusEvent::AnnounceAccepted(announce_hash));
                self.output
                    .push_back(ConsensusEvent::ComputeAnnounce(announce));
            }
        }

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
                block: *block,
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

        let block = *block;
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
                block,
                producer,
                chain,
                waiting_request: request,
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

            self.process_after_propagation(block, producer)?;
        }

        Ok(())
    }

    fn receive_computed_announce(&mut self, _announce_hash: HashOf<Announce>) -> Result<()> {
        Ok(())
    }

    fn receive_announce(&mut self, announce: VerifiedAnnounce) -> Result<()> {
        let (announce, sender) = announce.clone().into_parts();
        let sender = sender.to_address();

        if let State::WaitingForAnnounce { block, producer } = &self.state
            && sender == *producer
            && announce.block_hash == block.hash
        {
            self.process_announce_from_producer(announce, *producer)?;
            self.state = State::WaitingForBlock;
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

    fn receive_announces_response(&mut self, response: AnnouncesResponse) -> Result<()> {
        let State::WaitingForMissingAnnounces {
            block,
            producer,
            chain,
            waiting_request,
        } = &mut self.state
        else {
            return Ok(());
        };

        let block = *block;
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

        self.process_after_propagation(block, producer)?;

        Ok(())
    }
}

impl Stream for ConnectService {
    type Item = Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(Ok(event)))
        } else {
            Poll::Pending
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
    use ethexe_common::{HashOf, ValidatorsVec, mock::*};
    use ethexe_db::Database;
    use gsigner::{PrivateKey, PublicKey, SignedData};

    #[test]
    fn announce_not_computed_after_pending_and_rejected() {
        let validator_private_key = PrivateKey::random();
        let validator_address = PublicKey::from(&validator_private_key).to_address();
        let validators = ValidatorsVec::try_from(vec![validator_address]).unwrap();

        let db = Database::memory();
        let chain = BlockChain::mock((10, validators)).setup(&db);

        let mut service = ConnectService::new(db, Duration::from_secs(12), 10);
        service
            .receive_new_chain_head(chain.blocks[10].to_simple())
            .unwrap();
        service.receive_synced_block(chain.blocks[10].hash).unwrap();

        // send announce with unknown parent and in state when announce should be pending
        let announce = Announce {
            block_hash: chain.blocks[10].hash,
            parent: HashOf::random(),
            gas_allowance: Some(199),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();
        service
            .receive_announce(
                SignedData::create(&validator_private_key, announce.clone())
                    .unwrap()
                    .into_verified(),
            )
            .unwrap();

        service
            .receive_prepared_block(chain.blocks[10].hash)
            .unwrap();

        assert_eq!(
            service.output,
            vec![ConsensusEvent::AnnounceRejected(announce_hash)]
        )
    }
}
