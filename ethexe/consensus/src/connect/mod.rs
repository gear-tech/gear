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
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService, SignedProducerBlock,
    SignedValidationRequest,
};
use anyhow::Result;
use ethexe_common::SimpleBlockData;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

/// Consensus service which tracks the on-chain and ethexe events
/// in order to keep the program states in local database actual.
#[derive(Debug, Default)]
pub struct SimpleConnectService {
    chain_head: Option<SimpleBlockData>,
    output: VecDeque<ConsensusEvent>,
}

impl SimpleConnectService {
    /// Creates a new instance of `SimpleConnectService`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ConsensusService for SimpleConnectService {
    fn role(&self) -> String {
        "Connect".to_string()
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.chain_head = Some(block);

        Ok(())
    }

    fn receive_synced_block(&mut self, block: H256) -> Result<()> {
        let Some(block_data) = self.chain_head.as_ref() else {
            self.output.push_back(ConsensusEvent::Warning(format!(
                "Received synced block {block}, but no chain-head was received yet",
            )));

            return Ok(());
        };

        if block_data.hash != block {
            self.output.push_back(ConsensusEvent::Warning(format!(
                "Received synced block {block} is different from the expected block hash {}",
                block_data.hash
            )));

            return Ok(());
        }

        self.output
            .push_back(ConsensusEvent::ComputeBlock(block_data.hash));

        Ok(())
    }

    fn receive_computed_block(&mut self, _block_hash: H256) -> Result<()> {
        Ok(())
    }

    fn receive_block_from_producer(&mut self, _block_hash: SignedProducerBlock) -> Result<()> {
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
    type Item = anyhow::Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(Ok(event)))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for SimpleConnectService {
    fn is_terminated(&self) -> bool {
        false
    }
}
