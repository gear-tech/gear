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
    announces::{self, AnnounceStatus, DBExt},
    utils,
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::OnChainStorageRO,
    network::{AnnouncesRequest, CheckedAnnouncesResponse},
};
use ethexe_db::Database;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    mem,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use uluru::LRUCache;

/// Maximum number of pending announces to store
const MAX_PENDING_ANNOUNCES: usize = NonZeroUsize::new(10).unwrap().get();

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
    pending_announces: LRUCache<Option<(Announce, Address)>, MAX_PENDING_ANNOUNCES>,
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
            pending_announces: LRUCache::new(),
            output: VecDeque::new(),
        }
    }

    fn process_after_propagation(&mut self, block: SimpleBlockData, producer: Address) {
        if let Some(announce) = self
            .pending_announces
            .find(|v| {
                v.as_ref()
                    .map(|(announce, sender)| {
                        *sender == producer && announce.block_hash == block.hash
                    })
                    .unwrap_or(false)
            })
            .and_then(|v| v.take().map(|v| v.0))
        {
            self.output
                .push_back(ConsensusEvent::ComputeAnnounce(announce));
            self.state = State::WaitingForBlock;
        } else {
            self.state = State::WaitingForAnnounce { block, producer };
        }
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
            let validators = self.db.block_validators(block_hash).ok_or(anyhow!(
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
            match announces::accept_announce(
                &self.db,
                announce.clone(),
                self.commitment_delay_limit,
            )? {
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
            self.pending_announces.insert(Some((announce, sender)));
        }

        Ok(())
    }

    fn receive_validation_request(&mut self, _batch: VerifiedValidationRequest) -> Result<()> {
        Ok(())
    }

    fn receive_validation_reply(&mut self, _reply: BatchCommitmentValidationReply) -> Result<()> {
        Ok(())
    }

    fn receive_announces_response(&mut self, response: CheckedAnnouncesResponse) -> Result<()> {
        let State::WaitingForMissingAnnounces {
            block,
            producer,
            chain,
            waiting_request,
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
