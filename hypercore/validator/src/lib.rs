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
use hypercore_network::service::NetworkGossip;
use hypercore_processor::LocalOutcome;
use hypercore_sequencer::{AggregatedCommitments, CodeCommitment};
use hypercore_signer::{Address, PublicKey, Signer};
use parity_scale_codec::Encode;
use std::sync::Arc;

pub struct Config {
    pub pub_key: PublicKey,
    pub router_address: Address,
}

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    current_codes: Vec<CodeCommitment>,
    router_address: Address,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            current_codes: vec![],
            router_address: config.router_address,
        }
    }

    pub fn has_commit(&self) -> bool {
        !self.current_codes.is_empty()
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

    pub fn push_commitment<N: NetworkGossip>(
        &mut self,
        network: Arc<N>,
        outcomes: &[LocalOutcome],
    ) -> Result<()> {
        // parse outcomes
        for outcome in outcomes {
            match outcome {
                LocalOutcome::CodeApproved(code_id) => {
                    self.current_codes.push(CodeCommitment {
                        code_id: *code_id,
                        approved: true,
                    });
                }
                LocalOutcome::CodeRejected(code_id) => {
                    self.current_codes.push(CodeCommitment {
                        code_id: *code_id,
                        approved: false,
                    });
                }
            }
        }

        let origin = self.pub_key.to_address();

        // broadcast aggregated_code_commitments to the network peers
        network.broadcast_commitments((origin, self.codes_aggregation()?).encode());

        Ok(())
    }

    pub fn clear(&mut self) {
        self.current_codes.clear();
    }
}
