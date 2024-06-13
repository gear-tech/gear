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
use hypercore_signer::{Address, PublicKey, Signer};

pub use agro::{AggregatedCommitments, CodeCommitment};

pub struct Config {
    pub ethereum_rpc: String,
    pub sign_tx_public: PublicKey,
    pub router_address: Address,
}

#[allow(unused)]
pub struct Sequencer {
    signer: Signer,
    ethereum_rpc: String,
    key: PublicKey,
    codes_aggregation: Aggregator<CodeCommitment>,
    router_address: Address,
}

impl Sequencer {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            ethereum_rpc: config.ethereum_rpc.clone(),
            codes_aggregation: Aggregator::new(1),
            key: config.sign_tx_public,
            router_address: config.router_address,
        }
    }

    async fn eth(&self) -> Result<hypercore_ethereum::HypercoreEthereum> {
        hypercore_ethereum::HypercoreEthereum::new(
            &self.ethereum_rpc,
            self.router_address,
            self.signer.clone(),
            self.key.to_address(),
        )
        .await
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Block {
                ref block_hash,
                parent_hash: _,
                block_number: _,
                timestamp: _,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");

                if self.codes_aggregation.len() > 0 {
                    log::debug!(
                        "Building on top of existing aggregation of {} commitments",
                        self.codes_aggregation.len()
                    );
                }
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
            log::debug!(
                "Collected some {0} code commitments. Trying to submit...",
                self.codes_aggregation.len()
            );
            let active_aggregation =
                core::mem::replace(&mut self.codes_aggregation, Aggregator::new(1));

            if let Some(code_commitments) = active_aggregation.find_root() {
                log::debug!("Achieved consensus on code commitments. Submitting...");
                self.submit_codes_commitment(code_commitments).await?;
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        }

        Ok(())
    }

    async fn submit_codes_commitment(
        &self,
        commitments: MultisignedCommitments<CodeCommitment>,
    ) -> Result<()> {
        log::debug!("Code commitment to submit: {commitments:?}");

        let codes = commitments
            .commitments
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let signatures = commitments.signatures;

        let router = self.eth().await?.router();
        if let Err(e) = router.commit_codes(codes, signatures).await {
            log::error!("Failed to commit code ids: {e}");
        }

        Ok(())
    }

    pub fn receive_codes_commitment(
        &mut self,
        origin: Address,
        commitments: AggregatedCommitments<CodeCommitment>,
    ) -> Result<()> {
        log::debug!("Received codes commitment from {}", origin);
        self.codes_aggregation.push(origin, commitments);
        Ok(())
    }
}
