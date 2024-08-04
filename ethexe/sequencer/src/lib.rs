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
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Not,
};
use tokio::sync::watch;

pub use agro::AggregatedCommitments;

pub struct Config {
    pub ethereum_rpc: String,
    pub sign_tx_public: PublicKey,
    pub router_address: Address,
    pub validators: Vec<Address>,
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

    code_commitments: BTreeMap<Digest, (CodeCommitment, u64)>,
    block_commitments: BTreeMap<Digest, (BlockCommitment, u64)>,

    codes_aggregator: BTreeMap<Digest, MultisignedCommitments<CodeCommitment>>,
    blocks_aggregator: BTreeMap<Digest, MultisignedCommitments<BlockCommitment>>,

    status: SequencerStatus,
    status_sender: watch::Sender<SequencerStatus>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub allowed_prev_commitment_hash: H256,
    pub transitions_digest: Digest,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        Self {
            block_hash: commitment.block_hash,
            allowed_pred_block_hash: commitment.allowed_pred_block_hash,
            allowed_prev_commitment_hash: commitment.allowed_prev_commitment_hash,
            transitions_digest: commitment.transitions.as_digest(),
        }
    }
}

impl AsDigest for BlockCommitmentValidationRequest {
    fn as_digest(&self) -> Digest {
        let mut message = Vec::with_capacity(3 * size_of::<H256>() + size_of::<Digest>());

        message.extend_from_slice(self.block_hash.as_bytes());
        message.extend_from_slice(self.allowed_pred_block_hash.as_bytes());
        message.extend_from_slice(self.allowed_prev_commitment_hash.as_bytes());
        message.extend_from_slice(&self.transitions_digest);

        message.as_digest()
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
        codes: BTreeMap<Digest, CodeCommitment>,
        blocks: BTreeMap<Digest, BlockCommitmentValidationRequest>,
    },
    ApproveCommitments {
        origin: Address,
        codes: Option<(Digest, Signature)>,
        blocks: Option<(Digest, Signature)>,
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
            validators: config.validators.iter().cloned().collect(),
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
        if let Event::Block(data) = event {
            log::debug!("Receive block {:?}", data.block_hash);

            self.update_status(|status| {
                *status = SequencerStatus::default();
            });
        }

        Ok(())
    }

    pub fn process_collected_commitments(&mut self) -> Result<(Option<Digest>, Option<Digest>)> {
        let codes_digest = Self::pop_suitable_commitments(
            &mut self.code_commitments,
            &mut self.codes_aggregator,
            self.threshold,
        );

        let blocks_digest = Self::pop_suitable_commitments(
            &mut self.block_commitments,
            &mut self.blocks_aggregator,
            self.threshold,
        );

        Ok((codes_digest, blocks_digest))
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

    pub fn receive_code_commitments(
        &mut self,
        origin: Address,
        aggregated: AggregatedCommitments<CodeCommitment>,
    ) -> Result<()> {
        Self::receive_commitments(
            origin,
            aggregated,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.code_commitments,
        )
    }

    pub fn receive_block_commitments(
        &mut self,
        origin: Address,
        aggregated: AggregatedCommitments<BlockCommitment>,
    ) -> Result<()> {
        Self::receive_commitments(
            origin,
            aggregated,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.block_commitments,
        )
    }

    pub fn receive_codes_signature(
        &mut self,
        origin: Address,
        digest: Digest,
        signature: Signature,
    ) -> Result<()> {
        Self::receive_signature(
            origin,
            digest,
            signature,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.codes_aggregator,
        )
    }

    pub fn receive_blocks_signature(
        &mut self,
        origin: Address,
        digest: Digest,
        signature: Signature,
    ) -> Result<()> {
        Self::receive_signature(
            origin,
            digest,
            signature,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.blocks_aggregator,
        )
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }

    pub fn get_status_receiver(&self) -> watch::Receiver<SequencerStatus> {
        self.status_sender.subscribe()
    }

    pub fn get_multisigned_code_commitments(&self, digest: Digest) -> Option<&[CodeCommitment]> {
        self.codes_aggregator
            .get(&digest)
            .map(|multisigned| multisigned.commitments.as_slice())
    }

    pub fn get_multisigned_block_commitments(&self, digest: Digest) -> Option<&[BlockCommitment]> {
        self.blocks_aggregator
            .get(&digest)
            .map(|multisigned| multisigned.commitments.as_slice())
    }

    fn pop_suitable_commitments<C: AsDigest>(
        commitments: &mut BTreeMap<Digest, (C, u64)>,
        aggregator: &mut BTreeMap<Digest, MultisignedCommitments<C>>,
        threshold: u64,
    ) -> Option<Digest> {
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

        let digest = suitable_commitments.as_digest();

        aggregator.insert(
            digest,
            MultisignedCommitments {
                commitments: suitable_commitments,
                sources: Vec::new(),
                signatures: Vec::new(),
            },
        );

        Some(digest)
    }

    fn process_multisigned_candidate<C: AsDigest>(
        aggregator: &mut BTreeMap<Digest, MultisignedCommitments<C>>,
        threshold: u64,
    ) -> Option<MultisignedCommitments<C>> {
        let candidate = aggregator.iter().find_map(|(&digest, multisigned)| {
            (multisigned.sources.len() >= threshold as usize).then_some(digest)
        })?;

        let multisigned = aggregator
            .remove(&candidate)
            .unwrap_or_else(|| unreachable!("Must be in the map"));

        if multisigned.commitments.is_empty() {
            unreachable!("Guarantied to be not empty");
        }

        Some(multisigned)
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

    fn receive_commitments<C: AsDigest>(
        origin: Address,
        aggregated: AggregatedCommitments<C>,
        validators: &HashSet<Address>,
        router_address: Address,
        commitments_storage: &mut BTreeMap<Digest, (C, u64)>,
    ) -> Result<()> {
        if validators.contains(&origin).not() {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if aggregated.verify_origin(router_address, origin)?.not() {
            return Err(anyhow!("Signature verification failed for {origin}"));
        }

        let mut processed = HashSet::new();
        for commitment in aggregated.commitments {
            let digest = commitment.as_digest();
            if processed.contains(&digest) {
                continue;
            }
            processed.insert(digest);
            let (_, signatures_amount) = commitments_storage
                .entry(digest)
                .or_insert_with(|| (commitment, 0));
            *signatures_amount += 1;
        }

        Ok(())
    }

    fn receive_signature<C: AsDigest>(
        origin: Address,
        digest: Digest,
        signature: Signature,
        validators: &HashSet<Address>,
        router_address: Address,
        aggregator: &mut BTreeMap<Digest, MultisignedCommitments<C>>,
    ) -> Result<()> {
        if !validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if AggregatedCommitments::<C>::recover_digest(digest, signature.clone(), router_address)?
            != origin
        {
            return Err(anyhow!("Invalid signature"));
        }

        let multisigned = aggregator
            .get_mut(&digest)
            .ok_or(anyhow!("Aggregated commitment {digest:?} not found"))?;

        multisigned.sources.push(origin);
        multisigned.signatures.push(signature);

        Ok(())
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
