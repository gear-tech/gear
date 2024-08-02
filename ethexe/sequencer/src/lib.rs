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

use agro::MultisignedCommitments;
use anyhow::{anyhow, Result};
use ethexe_common::{BlockCommitment, CodeCommitment};
use ethexe_ethereum::Ethereum;
use ethexe_observer::Event;
use ethexe_signer::{Address, PublicKey, Signature, Signer};
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use std::collections::{BTreeMap, HashSet};
use tokio::sync::watch;

pub use agro::{AggregatedCommitments, SeqHash};

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
    key: PublicKey,
    ethereum: Ethereum,

    validators: HashSet<Address>,
    threshold: u64,

    code_commitments: BTreeMap<H256, (CodeCommitment, u64)>,
    block_commitments: BTreeMap<H256, (BlockCommitment, u64)>,

    codes_aggregator: BTreeMap<H256, MultisignedCommitments<CodeCommitment>>,
    blocks_aggregator: BTreeMap<H256, MultisignedCommitments<BlockCommitment>>,

    status: SequencerStatus,
    status_sender: watch::Sender<SequencerStatus>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub allowed_prev_commitment_hash: H256,
    pub transitions_hash: H256,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        Self {
            block_hash: commitment.block_hash,
            allowed_pred_block_hash: commitment.allowed_pred_block_hash,
            allowed_prev_commitment_hash: commitment.allowed_prev_commitment_hash,
            transitions_hash: commitment.transitions.hash(),
        }
    }
}

impl SeqHash for BlockCommitmentValidationRequest {
    fn hash(&self) -> H256 {
        [
            self.block_hash,
            self.allowed_pred_block_hash,
            self.allowed_prev_commitment_hash,
            self.transitions_hash,
        ]
        .as_ref()
        .hash()
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CodeCommitmentValidationRequest {
    pub code_id: CodeId,
    pub approved: bool,
}

impl From<&CodeCommitment> for CodeCommitmentValidationRequest {
    fn from(commitment: &CodeCommitment) -> Self {
        Self {
            code_id: commitment.code_id,
            approved: commitment.approved,
        }
    }
}

impl SeqHash for CodeCommitmentValidationRequest {
    fn hash(&self) -> H256 {
        CodeCommitment {
            code_id: self.code_id,
            approved: self.approved,
        }
        .hash()
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum NetworkMessage {
    PublishCommitments {
        origin: Address,
        codes: Option<AggregatedCommitments<CodeCommitment>>,
        blocks: Option<AggregatedCommitments<BlockCommitment>>,
    },
    RequestCommitmentsValidation {
        codes: BTreeMap<H256, CodeCommitmentValidationRequest>,
        blocks: BTreeMap<H256, BlockCommitmentValidationRequest>,
    },
    ApproveCommitments {
        origin: Address,
        codes: Option<(H256, Signature)>,
        blocks: Option<(H256, Signature)>,
    },
}

impl Sequencer {
    pub async fn new(config: &Config, signer: Signer) -> Result<Self> {
        let (status_sender, _status_receiver) = watch::channel(SequencerStatus::default());
        Ok(Self {
            signer: signer.clone(),
            key: config.sign_tx_public,
            ethereum: Ethereum::new(
                &config.ethereum_rpc,
                config.router_address,
                signer,
                config.sign_tx_public.to_address(),
            )
            .await?,
            validators: Default::default(),
            threshold: 1,
            code_commitments: BTreeMap::new(),
            block_commitments: BTreeMap::new(),
            codes_aggregator: BTreeMap::new(),
            blocks_aggregator: BTreeMap::new(),
            status: SequencerStatus::default(),
            status_sender,
        })
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Block(data) => {
                log::debug!("Receive block {:?}", data.block_hash);

                self.update_status(|status| {
                    *status = SequencerStatus::default();
                });
            }
            _ => {}
        }

        Ok(())
    }

    fn pop_suitable_commitments<C: SeqHash>(
        commitments: &mut BTreeMap<H256, (C, u64)>,
        aggregator: &mut BTreeMap<H256, MultisignedCommitments<C>>,
        threshold: u64,
    ) -> Option<H256> {
        let suitable_commitment_hashes: Vec<_> = commitments
            .iter()
            .filter_map(|(&hash, (_, amount))| (*amount >= threshold).then_some(hash))
            .collect();

        if suitable_commitment_hashes.is_empty() {
            return None;
        }

        let mut suitable_commitments = Vec::new();
        for hash in suitable_commitment_hashes.iter() {
            let (commitment, _) = commitments
                .remove(hash)
                .unwrap_or_else(|| unreachable!("Must be in the map"));
            suitable_commitments.push(commitment);
        }

        let hash = suitable_commitments.hash();

        aggregator.insert(
            hash,
            MultisignedCommitments {
                commitments: suitable_commitments,
                sources: Vec::new(),
                signatures: Vec::new(),
            },
        );

        Some(hash)
    }

    pub fn process_collected_commitments(&mut self) -> Result<(Option<H256>, Option<H256>)> {
        let codes_hash = Self::pop_suitable_commitments(
            &mut self.code_commitments,
            &mut self.codes_aggregator,
            self.threshold,
        );

        let blocks_hash = Self::pop_suitable_commitments(
            &mut self.block_commitments,
            &mut self.blocks_aggregator,
            self.threshold,
        );

        Ok((codes_hash, blocks_hash))
    }

    pub fn get_multisigned_code_commitments(&self, hash: H256) -> Option<&[CodeCommitment]> {
        self.codes_aggregator
            .get(&hash)
            .map(|multisigned| multisigned.commitments.as_slice())
    }

    pub fn get_multisigned_block_commitments(&self, hash: H256) -> Option<&[BlockCommitment]> {
        self.blocks_aggregator
            .get(&hash)
            .map(|multisigned| multisigned.commitments.as_slice())
    }

    fn process_multisigned_candidate<C: SeqHash>(
        aggregator: &mut BTreeMap<H256, MultisignedCommitments<C>>,
        threshold: u64,
    ) -> Option<MultisignedCommitments<C>> {
        let candidate = aggregator.iter().find_map(|(&hash, multisigned)| {
            (multisigned.sources.len() >= threshold as usize).then_some(hash)
        })?;

        let multisigned = aggregator
            .remove(&candidate)
            .unwrap_or_else(|| unreachable!("Must be in the map"));

        if multisigned.commitments.len() == 0 {
            unreachable!("Guarantied to be not empty");
        }

        Some(multisigned)
    }

    pub async fn submit_multisigned_commitments(&mut self) -> Result<()> {
        let mut codes_future = None;
        let mut blocks_future = None;
        let mut code_commitments_len = 0;
        let mut block_commitments_len = 0;

        let codes_candidate =
            Self::process_multisigned_candidate(&mut self.codes_aggregator, self.threshold);
        let blocks_candidate =
            Self::process_multisigned_candidate(&mut self.blocks_aggregator, self.threshold);

        if let Some(candidate) = codes_candidate {
            log::debug!(
                "Collected some {0} code commitments. Trying to submit...",
                candidate.commitments.len()
            );

            code_commitments_len = candidate.commitments.len() as u64;
            codes_future = Some(self.submit_codes_commitment(candidate));
        };

        if let Some(candidate) = blocks_candidate {
            log::debug!(
                "Collected some {0} transition commitments. Trying to submit...",
                candidate.commitments.len()
            );

            block_commitments_len = candidate.commitments.len() as u64;
            blocks_future = Some(self.submit_block_commitments(candidate));
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

    pub fn receive_code_commitments(
        &mut self,
        origin: Address,
        aggregated: AggregatedCommitments<CodeCommitment>,
    ) -> Result<()> {
        if !self.validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if !aggregated.verify_origin(self.ethereum.router().address(), origin)? {
            return Err(anyhow!("Signature verification failed for {origin}"));
        }

        let mut processed = HashSet::new();
        for commitment in aggregated.commitments {
            let hash = commitment.hash();
            if processed.contains(&hash) {
                continue;
            }
            processed.insert(hash);
            let (_, signatures_amount) = self
                .code_commitments
                .entry(hash)
                .or_insert_with(|| (commitment, 0));
            *signatures_amount += 1;
        }

        Ok(())
    }

    pub fn receive_block_commitments(
        &mut self,
        origin: Address,
        aggregated: AggregatedCommitments<BlockCommitment>,
    ) -> Result<()> {
        log::debug!("Received transition commitment from {}", origin);
        if !self.validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }
        if !aggregated.verify_origin(self.ethereum.router().address(), origin)? {
            return Err(anyhow!("Signature verification failed for {origin}"));
        }

        let mut processed = HashSet::new();
        for commitment in aggregated.commitments {
            let hash = commitment.hash();
            if processed.contains(&hash) {
                continue;
            }
            processed.insert(hash);
            let (_, signatures_amount) = self
                .block_commitments
                .entry(hash)
                .or_insert_with(|| (commitment, 0));
            *signatures_amount += 1;
        }

        Ok(())
    }

    fn receive_signature<C: SeqHash>(
        origin: Address,
        aggregated_hash: H256,
        signature: Signature,
        validators: &HashSet<Address>,
        aggregator: &mut BTreeMap<H256, Option<MultisignedCommitments<C>>>,
    ) -> Result<()> {
        if !validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if signature
            .recover_digest(*aggregated_hash.as_fixed_bytes())?
            .to_address()
            != origin
        {
            return Err(anyhow!("Invalid signature"));
        }

        let multisigned = aggregator
            .get_mut(&aggregated_hash)
            .ok_or(anyhow!("Aggregated commitment {aggregated_hash} not found"))?
            .get_or_insert_with(|| MultisignedCommitments {
                commitments: Default::default(),
                sources: Vec::new(),
                signatures: Vec::new(),
            });

        multisigned.sources.push(origin);
        multisigned.signatures.push(signature);

        Ok(())
    }

    pub fn receive_codes_signature(
        &mut self,
        origin: Address,
        aggregated_hash: H256,
        signature: Signature,
    ) -> Result<()> {
        if !self.validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }
        if signature
            .recover_digest(*aggregated_hash.as_fixed_bytes())?
            .to_address()
            != origin
        {
            return Err(anyhow!("Invalid signature"));
        }

        let multisigned = self
            .codes_aggregator
            .get_mut(&aggregated_hash)
            .ok_or(anyhow!("Aggregated commitment {aggregated_hash} not found"))?;

        multisigned.sources.push(origin);
        multisigned.signatures.push(signature);

        Ok(())
    }

    pub fn receive_blocks_signature(
        &mut self,
        origin: Address,
        aggregated_hash: H256,
        signature: Signature,
    ) -> Result<()> {
        if !self.validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if signature
            .recover_digest(*aggregated_hash.as_fixed_bytes())?
            .to_address()
            != origin
        {
            return Err(anyhow!("Invalid signature"));
        }

        let multisigned = self
            .blocks_aggregator
            .get_mut(&aggregated_hash)
            .ok_or(anyhow!("Aggregated commitment {aggregated_hash} not found"))?;

        multisigned.sources.push(origin);
        multisigned.signatures.push(signature);

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
