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
use hypercore_ethereum::Ethereum;
use hypercore_observer::Event;
use hypercore_signer::{Address, PublicKey, Signer};
use std::mem;

pub use agro::{AggregatedCommitments, CodeCommitment, TransitionCommitment};

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
    transitions_aggregation: Aggregator<TransitionCommitment>,
    ethereum: Ethereum,
}

impl Sequencer {
    pub async fn new(config: &Config, signer: Signer) -> Result<Self> {
        Ok(Self {
            signer: signer.clone(),
            ethereum_rpc: config.ethereum_rpc.clone(),
            codes_aggregation: Aggregator::new(1),
            transitions_aggregation: Aggregator::new(1),
            key: config.sign_tx_public,
            ethereum: Ethereum::new(
                &config.ethereum_rpc,
                config.router_address,
                signer,
                config.sign_tx_public.to_address(),
            )
            .await?,
        })
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Block(data) => {
                log::debug!(
                    "Processing events for {:?} (parent: {:?})",
                    data.block_hash,
                    data.parent_hash
                );

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

        let mut codes_future = None;
        let mut transitions_future = None;

        let codes_aggregation = mem::replace(&mut self.codes_aggregation, Aggregator::new(1));
        let transitions_aggregation =
            mem::replace(&mut self.transitions_aggregation, Aggregator::new(1));

        if codes_aggregation.len() > 0 {
            log::debug!(
                "Collected some {0} code commitments. Trying to submit...",
                codes_aggregation.len()
            );

            if let Some(code_commitments) = codes_aggregation.find_root() {
                log::debug!("Achieved consensus on code commitments. Submitting...");

                codes_future = Some(self.submit_codes_commitment(code_commitments));
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        };

        if transitions_aggregation.len() > 0 {
            log::debug!(
                "Collected some {0} transition commitments. Trying to submit...",
                transitions_aggregation.len()
            );

            if let Some(transition_commitments) = transitions_aggregation.find_root() {
                log::debug!("Achieved consensus on transition commitments. Submitting...");

                transitions_future =
                    Some(self.submit_transitions_commitment(transition_commitments));
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        };

        match (codes_future, transitions_future) {
            (Some(codes_future), Some(transitions_future)) => {
                let (codes_tx, transitions_tx) = futures::join!(codes_future, transitions_future);
                codes_tx?;
                transitions_tx?;
            }
            (Some(codes_future), None) => codes_future.await?,
            (None, Some(transitions_future)) => transitions_future.await?,
            (None, None) => {}
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

        let router = self.ethereum.router();
        if let Err(e) = router.commit_codes(codes, signatures).await {
            // TODO: return error?
            log::error!("Failed to commit code ids: {e}");
        }

        Ok(())
    }

    async fn submit_transitions_commitment(
        &self,
        commitments: MultisignedCommitments<TransitionCommitment>,
    ) -> Result<()> {
        log::debug!("Transition commitment to submit: {commitments:?}");

        let transitions = commitments
            .commitments
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let signatures = commitments.signatures;

        let router = self.ethereum.router();
        if let Err(e) = router.commit_transitions(transitions, signatures).await {
            // TODO: return error?
            log::error!("Failed to commit transitions: {e}");
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

    pub fn receive_transitions_commitment(
        &mut self,
        origin: Address,
        commitments: AggregatedCommitments<TransitionCommitment>,
    ) -> Result<()> {
        log::debug!("Received transition commitment from {}", origin);
        self.transitions_aggregation.push(origin, commitments);
        Ok(())
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }
}
