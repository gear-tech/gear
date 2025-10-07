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
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService, SignedAnnounce,
    SignedValidationRequest, manager::ValidatorsManager, utils,
};
use anyhow::Result;
use ethexe_common::{Address, AnnounceHash, SimpleBlockData, ValidatorsVec};
use ethexe_db::Database;
use ethexe_ethereum::router::ValidatorsProvider;
use futures::{FutureExt, Stream, future::BoxFuture, stream::FusedStream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

const MAX_PENDING_ANNOUNCES: usize = 10;
const _: () = assert!(
    MAX_PENDING_ANNOUNCES != 0,
    "MAX_PENDING_ANNOUNCES must not be zero"
);

#[allow(clippy::enum_variant_names)]
#[derive(derive_more::Debug)]
enum State {
    WaitingForBlock,
    WaitingForBlockValidators {
        block: SimpleBlockData,
        #[debug(skip)]
        future: BoxFuture<'static, Result<ValidatorsVec>>,

        // block_synced: bool,
        block_prepared: bool,
    },
    // WaitingForSyncedBlock {
    //     block: SimpleBlockData,
    //     producer: Address,
    // },
    WaitingForPreparedBlock {
        block: SimpleBlockData,
        producer: Address,
    },
    WaitingForAnnounce {
        block: SimpleBlockData,
        producer: Address,
    },
}

/// Consensus service which tracks the on-chain and ethexe events
/// in order to keep the program states in local database actual.
#[derive(derive_more::Debug)]
pub struct SimpleConnectService {
    // #[debug(skip)]
    // db: Database,
    slot_duration: Duration,

    state: State,
    pending_announces: VecDeque<SignedAnnounce>,
    output: VecDeque<ConsensusEvent>,

    validators_manager: ValidatorsManager<Database>,
}

impl SimpleConnectService {
    /// Creates a new instance of `SimpleConnectService`.
    pub fn new<V>(db: Database, slot_duration: Duration, validators_provider: V) -> Self
    where
        V: ValidatorsProvider + 'static,
    {
        Self {
            // db: db.clone(),
            slot_duration,
            state: State::WaitingForBlock,
            pending_announces: VecDeque::with_capacity(MAX_PENDING_ANNOUNCES),
            output: VecDeque::new(),
            validators_manager: ValidatorsManager::new(db, validators_provider),
        }
    }
}

impl ConsensusService for SimpleConnectService {
    fn role(&self) -> String {
        "Connect".to_string()
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        // self.state = State::WaitingForSyncedBlock { block };
        self.state = State::WaitingForBlockValidators {
            block,
            future: self
                .validators_manager
                .clone()
                .get_validators(block.hash)
                .boxed(),
            block_prepared: false,
        };
        Ok(())
    }

    fn receive_synced_block(&mut self, _block_hash: H256) -> Result<()> {
        // match &mut self.state {
        //     State::WaitingForBlockValidators { block_synced, .. } => {
        //         block_synced = true;
        //     }
        //     State::WaitingForSyncedBlock { block, .. } => {}
        //     _ => Ok(()),
        // }
        // if let State::WaitingForSyncedBlock { block, .. } = &self.state
        //     && block.hash == block_hash
        // {
        //     let validators = Default::default();
        //     let producer = utils::block_producer_for(
        //         &validators,
        //         block.header.timestamp,
        //         self.slot_duration.as_secs(),
        //     );

        //     self.state = State::WaitingForPreparedBlock {
        //         block: block.clone(),
        //         producer,
        //     };
        // }

        Ok(())
    }

    fn receive_prepared_block(&mut self, prepared_block_hash: H256) -> Result<()> {
        if let State::WaitingForBlockValidators {
            block,
            block_prepared,
            ..
        } = &mut self.state
            && block.hash == prepared_block_hash
        {
            *block_prepared = true
        };

        if let State::WaitingForPreparedBlock { block, producer } = &self.state
            && block.hash == prepared_block_hash
        {
            self.handle_prepared_block(*block, *producer);
        }

        Ok(())
    }

    fn receive_computed_announce(&mut self, _announce: AnnounceHash) -> Result<()> {
        Ok(())
    }

    fn receive_announce(&mut self, announce: SignedAnnounce) -> Result<()> {
        debug_assert!(
            self.pending_announces.len() <= MAX_PENDING_ANNOUNCES,
            "Logically impossible to have more than {MAX_PENDING_ANNOUNCES} pending announces because oldest ones are dropped"
        );

        if let State::WaitingForAnnounce { block, producer } = &self.state
            && announce.address() == *producer
            && announce.data().block_hash == block.hash
        {
            let (announce, _) = announce.into_parts();
            self.output
                .push_back(ConsensusEvent::ComputeAnnounce(announce));
            self.state = State::WaitingForBlock;
            return Ok(());
        }

        if self.pending_announces.len() == MAX_PENDING_ANNOUNCES {
            let old_announce = self.pending_announces.pop_front().unwrap();
            tracing::trace!(
                "Pending announces limit reached, dropping oldest announce: {:?} from {}",
                old_announce.data(),
                old_announce.address()
            );
        }

        self.pending_announces.push_back(announce);

        Ok(())
    }

    fn receive_validation_request(&mut self, _signed_batch: SignedValidationRequest) -> Result<()> {
        Ok(())
    }

    fn receive_validation_reply(&mut self, _reply: BatchCommitmentValidationReply) -> Result<()> {
        Ok(())
    }
}

impl Stream for SimpleConnectService {
    type Item = Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            return Poll::Ready(Some(Ok(event)));
        }

        let slot_duration = self.slot_duration.as_secs();
        if let State::WaitingForBlockValidators {
            block,
            future,
            block_prepared,
        } = &mut self.state
        {
            let validators = futures::ready!(future.poll_unpin(cx))?;
            let block = *block;
            let producer =
                utils::block_producer_for(&validators, block.header.timestamp, slot_duration);

            if *block_prepared {
                self.handle_prepared_block(block, producer);
            } else {
                self.state = State::WaitingForPreparedBlock { block, producer }
            }
        }
        Poll::Pending
    }
}

impl FusedStream for SimpleConnectService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl SimpleConnectService {
    fn handle_prepared_block(&mut self, block: SimpleBlockData, producer: Address) {
        if let Some(index) = self.pending_announces.iter().position(|announce| {
            announce.address() == producer && announce.data().block_hash == block.hash
        }) {
            let (announce, _) = self
                .pending_announces
                .remove(index)
                .expect("Index must be valid")
                .into_parts();
            self.output
                .push_back(ConsensusEvent::ComputeAnnounce(announce));
            self.state = State::WaitingForBlock;
        } else {
            self.state = State::WaitingForAnnounce { block, producer };
        };
    }
}
