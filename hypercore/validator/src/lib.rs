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
use gprimitives::H256;
use hypercore_network::service::NetworkGossip;
use hypercore_processor::LocalOutcome;
use hypercore_sequencer::{AggregatedCommitments, CodeHashCommitment};
use hypercore_signer::{PublicKey, Signer};
use parity_scale_codec::Encode;
use std::{collections::HashSet, sync::Arc};

pub struct Config {
    pub pub_key: PublicKey,
}

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    aggregated_code_commitments: HashSet<AggregatedCommitments<CodeHashCommitment>>,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            aggregated_code_commitments: Default::default(),
        }
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn aggregated_code_commitments(
        &mut self,
    ) -> &mut HashSet<AggregatedCommitments<CodeHashCommitment>> {
        &mut self.aggregated_code_commitments
    }

    pub fn push_commitment<N: NetworkGossip>(
        &mut self,
        network: Arc<N>,
        outcomes: &[LocalOutcome],
    ) -> Result<()> {
        let mut code_commitments = Vec::new();

        // parse outcomes
        for outcome in outcomes {
            match outcome {
                LocalOutcome::CodeCommitment(code_id) => {
                    code_commitments.push(CodeHashCommitment(H256::from(code_id.into_bytes())))
                }
            }
        }

        let aggregated_code_commitments = AggregatedCommitments::aggregate_commitments(
            code_commitments,
            &self.signer,
            self.pub_key,
        )?;

        // TODO: store aggregates by hash to avoid unnecessary calculations and signing
        // if hash([commitments]) not in self.aggregated_code_commitments
        if self
            .aggregated_code_commitments
            .insert(aggregated_code_commitments.clone())
        {
            let origin = self.pub_key.to_address();

            // broadcast aggregated_code_commitments to the network peers
            network.broadcast_commitments((origin, aggregated_code_commitments).encode());
        }

        Ok(())
    }

    pub fn clear_commitments(&mut self) {
        self.aggregated_code_commitments.clear();
    }
}
