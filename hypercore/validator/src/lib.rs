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
use hypercore_sequencer::{AggregatedCommitments, CodeCommitment, TransitionCommitment};
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
    current_transitions: Vec<TransitionCommitment>,
    router_address: Address,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            current_codes: vec![],
            current_transitions: vec![],
            router_address: config.router_address,
        }
    }

    pub fn has_commit(&self) -> bool {
        !(self.current_codes.is_empty() || self.current_transitions.is_empty())
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

    pub fn transitions_aggregation(
        &mut self,
    ) -> Result<AggregatedCommitments<TransitionCommitment>> {
        AggregatedCommitments::aggregate_commitments(
            self.current_transitions.clone(),
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
                LocalOutcome::Transition {
                    program_id,
                    old_state_hash,
                    new_state_hash,
                    outgoing_messages,
                } => {
                    // TODO: they're about to be aggregated before signing: like commitments A0 -> A1 && A1 -> A2 will one single commit A0 -> A2.
                    self.current_transitions.push(TransitionCommitment {
                        program_id: *program_id,
                        old_state_hash: *old_state_hash,
                        new_state_hash: *new_state_hash,
                        outgoing_messages: outgoing_messages.clone(),
                    })
                }
            }
        }

        let origin = self.pub_key.to_address();

        // broadcast (aggregated_code_commitments, aggregated_transitions_commitments) to the network peers
        let commitments = (self.codes_aggregation()?, self.transitions_aggregation()?);
        network.broadcast_commitments((origin, commitments).encode());

        Ok(())
    }

    pub fn clear(&mut self) {
        self.current_codes.clear();
        self.current_transitions.clear();
    }
}
