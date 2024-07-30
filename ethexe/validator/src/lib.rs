// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::Result;
use ethexe_common::router::{BlockCommitment, CodeCommitment};
use ethexe_network::service::NetworkGossip;
use ethexe_sequencer::AggregatedCommitments;
use ethexe_signer::{Address, PublicKey, Signer};
use parity_scale_codec::Encode;
use std::sync::Arc;

pub enum Commitment {
    Code(CodeCommitment),
    Block(BlockCommitment),
}

pub struct Config {
    pub pub_key: PublicKey,
    pub router_address: Address,
}

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    current_codes: Vec<CodeCommitment>,
    current_blocks: Vec<BlockCommitment>,
    router_address: Address,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            current_codes: vec![],
            current_blocks: vec![],
            router_address: config.router_address,
        }
    }

    pub fn has_codes_commit(&self) -> bool {
        !self.current_codes.is_empty()
    }

    pub fn has_transitions_commit(&self) -> bool {
        !self.current_blocks.is_empty()
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn codes_aggregation(&mut self) -> Result<AggregatedCommitments<CodeCommitment>> {
        AggregatedCommitments::aggregate_commitments(
            self.current_codes.clone(),
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn blocks_aggregation(&mut self) -> Result<AggregatedCommitments<BlockCommitment>> {
        AggregatedCommitments::aggregate_commitments(
            self.current_blocks.clone(),
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn push_commitments<N: NetworkGossip>(
        &mut self,
        network: Arc<N>,
        commitments: Vec<Commitment>,
    ) -> Result<()> {
        for commitment in commitments {
            match commitment {
                Commitment::Code(code_commitment) => self.current_codes.push(code_commitment),
                Commitment::Block(block_commitment) => self.current_blocks.push(block_commitment),
            }
        }

        let origin = self.pub_key.to_address();

        // broadcast (aggregated_code_commitments, aggregated_transitions_commitments) to the network peers
        let commitments = (self.codes_aggregation()?, self.blocks_aggregation()?);
        network.broadcast_commitments((origin, commitments).encode());

        Ok(())
    }

    pub fn clear(&mut self) {
        self.current_codes.clear();
        self.current_blocks.clear();
    }

    pub fn address(&self) -> Address {
        self.pub_key.to_address()
    }
}
