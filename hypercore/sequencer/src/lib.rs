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

//! Sequencer for hypercore.

mod agro;

use agro::{Aggregator, MultisignedCommitments};
use anyhow::Result;
use hypercore_observer::Event;
use hypercore_signer::{Address, Signer};

pub use agro::{AggregatedCommitments, CodeHashCommitment};

pub struct Config {
    pub ethereum_rpc: String,
    pub sign_tx_public: String,
}

pub struct Sequencer {
    signer: Signer,
    ethereum_rpc: String,

    codes_aggregation: Aggregator<CodeHashCommitment>,
}

impl Sequencer {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            ethereum_rpc: config.ethereum_rpc.clone(),
            codes_aggregation: Aggregator::new(1),
        }
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Block {
                ref block_hash,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");

                log::debug!("Restarting upload code aggregation");
                let previous_aggregation =
                    core::mem::replace(&mut self.codes_aggregation, Aggregator::new(1));
                log::debug!(
                    "Dropped previous aggregation of {} commitments",
                    previous_aggregation.len()
                )
            }
            Event::UploadCode { code_id, .. } => {
                log::debug!("Observed code_hash#{:?}. Waiting for inclusion...", code_id);
            }
        }

        Ok(())
    }

    pub async fn process_block_timeout(&mut self) -> Result<()> {
        log::debug!("Block timeout reached. Submitting aggregated commitments");

        if self.codes_aggregation.len() > 0 {
            log::debug!("Collected some code commitments. Trying to submit...");
            let active_aggregation =
                core::mem::replace(&mut self.codes_aggregation, Aggregator::new(1));

            if let Some(code_commitments) = active_aggregation.find_root() {
                log::debug!("Achieved consensus on code commitments. Submitting...");
                Self::submit_codes_commitment(code_commitments).await?;
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        }

        Ok(())
    }

    async fn submit_codes_commitment(
        _commitments: MultisignedCommitments<CodeHashCommitment>,
    ) -> Result<()> {
        // TODO

        Ok(())
    }

    pub fn receive_codes_commitment(
        &mut self,
        origin: Address,
        commitments: AggregatedCommitments<CodeHashCommitment>,
    ) -> Result<()> {
        log::debug!("Received codes commitment from {}", origin);
        self.codes_aggregation.push(origin, commitments);
        Ok(())
    }
}
