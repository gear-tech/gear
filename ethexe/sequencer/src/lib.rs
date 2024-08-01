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

//! Sequencer for ethexe.

mod agro;

use agro::{Aggregator, MultisignedCommitments};
use anyhow::Result;
use ethexe_common::{BlockCommitment, CodeCommitment};
use ethexe_ethereum::Ethereum;
use ethexe_observer::Event;
use ethexe_signer::{Address, PublicKey, Signer};
use std::mem;
use tokio::sync::watch;

pub use agro::AggregatedCommitments;

pub struct Config {
    pub ethereum_rpc: String,
    pub sign_tx_public: PublicKey,
    pub router_address: Address,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SequencerStatus {
    pub aggregated_commitments: u64,
    pub submitted_code_commitments: u64,
    pub submitted_block_commitments: u64,
}

#[allow(unused)]
pub struct Sequencer {
    signer: Signer,
    ethereum_rpc: String,
    key: PublicKey,
    codes_aggregation: Aggregator<CodeCommitment>,
    blocks_aggregation: Aggregator<BlockCommitment>,
    ethereum: Ethereum,
    status: SequencerStatus,
    status_sender: watch::Sender<SequencerStatus>,
}

impl Sequencer {
    pub async fn new(config: &Config, signer: Signer) -> Result<Self> {
        let (status_sender, _status_receiver) = watch::channel(SequencerStatus::default());
        Ok(Self {
            signer: signer.clone(),
            ethereum_rpc: config.ethereum_rpc.clone(),
            codes_aggregation: Aggregator::new(1),
            blocks_aggregation: Aggregator::new(1),
            key: config.sign_tx_public,
            ethereum: Ethereum::new(
                &config.ethereum_rpc,
                config.router_address,
                signer,
                config.sign_tx_public.to_address(),
            )
            .await?,
            status: SequencerStatus::default(),
            status_sender,
        })
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        self.update_status(|status| {
            *status = SequencerStatus::default();
        });
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
            Event::CodeLoaded(data) => {
                log::debug!(
                    "Observed code_hash#{:?}. Waiting for inclusion...",
                    data.code_id
                );
            }
        }

        Ok(())
    }

    pub async fn process_block_timeout(&mut self) -> Result<()> {
        log::debug!("Block timeout reached. Submitting aggregated commitments");

        let mut codes_future = None;
        let mut blocks_future = None;
        let mut code_commitments_len = 0;
        let mut block_commitments_len = 0;

        let codes_aggregation = mem::replace(&mut self.codes_aggregation, Aggregator::new(1));
        let blocks_aggregation = mem::replace(&mut self.blocks_aggregation, Aggregator::new(1));

        if codes_aggregation.len() > 0 {
            log::debug!(
                "Collected some {0} code commitments. Trying to submit...",
                codes_aggregation.len()
            );

            if let Some(code_commitments) = codes_aggregation.find_root() {
                log::debug!("Achieved consensus on code commitments. Submitting...");

                code_commitments_len = code_commitments.commitments.len() as u64;
                codes_future = Some(self.submit_codes_commitment(code_commitments));
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        };

        if blocks_aggregation.len() > 0 {
            log::debug!(
                "Collected some {0} transition commitments. Trying to submit...",
                blocks_aggregation.len()
            );

            if let Some(block_commitments) = blocks_aggregation.find_root() {
                log::debug!("Achieved consensus on transition commitments. Submitting...");
                block_commitments_len = block_commitments.commitments.len() as u64;
                blocks_future = Some(self.submit_block_commitments(block_commitments));
            } else {
                log::debug!("No consensus on code commitments found. Discarding...");
            }
        };

        match (codes_future, blocks_future) {
            (Some(codes_future), Some(transitions_future)) => {
                let (codes_tx, transitions_tx) = futures::join!(codes_future, transitions_future);
                codes_tx?;
                transitions_tx?;
            }
            (Some(codes_future), None) => codes_future.await?,
            (None, Some(transitions_future)) => transitions_future.await?,
            (None, None) => {}
        }

        self.update_status(|status| {
            status.submitted_code_commitments += code_commitments_len;
            status.submitted_block_commitments += block_commitments_len;
        });

        Ok(())
    }

    async fn submit_codes_commitment(
        &self,
        signed_commitments: MultisignedCommitments<CodeCommitment>,
    ) -> Result<()> {
        log::debug!("Code commitment to submit: {signed_commitments:?}");

        let codes = signed_commitments
            .commitments
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let signatures = signed_commitments.signatures;

        let router = self.ethereum.router();
        if let Err(e) = router.commit_codes(codes, signatures).await {
            // TODO: return error?
            log::error!("Failed to commit code ids: {e}");
        }

        Ok(())
    }

    async fn submit_block_commitments(
        &self,
        signed_commitments: MultisignedCommitments<BlockCommitment>,
    ) -> Result<()> {
        log::debug!("Transition commitment to submit: {signed_commitments:?}");

        let block_commitments = signed_commitments
            .commitments
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let signatures = signed_commitments.signatures;

        let router = self.ethereum.router();
        match router.commit_blocks(block_commitments, signatures).await {
            Err(e) => {
                // TODO: return error?
                log::error!("Failed to commit transitions: {e}");
            }
            Ok(tx_hash) => {
                log::info!(
                    "Blocks commitment transaction {tx_hash} was added to the pool successfully"
                );
            }
        }

        Ok(())
    }

    pub fn receive_codes_commitment(
        &mut self,
        origin: Address,
        commitments: AggregatedCommitments<CodeCommitment>,
    ) -> Result<()> {
        self.update_status(|status| {
            status.aggregated_commitments += 1;
        });
        log::debug!("Received codes commitment from {}", origin);
        self.codes_aggregation.push(origin, commitments);
        Ok(())
    }

    pub fn receive_block_commitment(
        &mut self,
        origin: Address,
        commitments: AggregatedCommitments<BlockCommitment>,
    ) -> Result<()> {
        log::debug!("Received transition commitment from {}", origin);
        self.blocks_aggregation.push(origin, commitments);
        Ok(())
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }

    pub fn get_status_receiver(&self) -> watch::Receiver<SequencerStatus> {
        self.status_sender.subscribe()
    }

    fn update_status<F>(&mut self, update_fn: F)
    where
        F: FnOnce(&mut SequencerStatus),
    {
        let mut status = self.status;
        update_fn(&mut status);
        let _ = self.status_sender.send_replace(status);
    }
}
