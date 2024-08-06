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

pub mod agro;

use agro::MultisignedCommitments;
use anyhow::{anyhow, Result};
use ethexe_common::{BlockCommitment, CodeCommitment};
use ethexe_ethereum::Ethereum;
use ethexe_observer::Event;
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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

type CommitmentsMap<C> = BTreeMap<Digest, (C, HashSet<Address>)>;
type MultisignedDigests = (BTreeSet<Digest>, HashMap<Address, Signature>);
type Candidate = (Digest, MultisignedDigests);

pub struct Sequencer {
    key: PublicKey,
    ethereum: Ethereum,

    validators: HashSet<Address>,
    threshold: u64,

    code_commitments: CommitmentsMap<CodeCommitment>,
    block_commitments: CommitmentsMap<BlockCommitment>,

    codes_candidate: Option<Candidate>,
    blocks_candidate: Option<Candidate>,

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
        Ok(Sequencer {
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
            code_commitments: Default::default(),
            block_commitments: Default::default(),
            codes_candidate: Default::default(),
            blocks_candidate: Default::default(),
            status: Default::default(),
            status_sender,
        })
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        if let Event::Block(data) = event {
            log::debug!("Receive block {:?}", data.block_hash);

            self.codes_candidate = None;
            self.blocks_candidate = None;

            self.update_status(|status| {
                *status = SequencerStatus::default();
            });
        }

        Ok(())
    }

    pub fn process_collected_commitments(&mut self) -> Result<(Option<Digest>, Option<Digest>)> {
        if self.codes_candidate.is_some() || self.blocks_candidate.is_some() {
            return Err(anyhow!("Previous commitments candidate are not submitted"));
        }

        let codes_digest = Self::set_candidate_commitments(
            &self.code_commitments,
            &mut self.codes_candidate,
            self.threshold,
        );

        let blocks_digest = Self::set_candidate_commitments(
            &self.block_commitments,
            &mut self.blocks_candidate,
            self.threshold,
        );

        Ok((codes_digest, blocks_digest))
    }

    pub async fn submit_multisigned_commitments(&mut self) -> Result<()> {
        let mut codes_future = None;
        let mut blocks_future = None;
        let mut code_commitments_len = 0;
        let mut block_commitments_len = 0;

        let codes_candidate = Self::process_multisigned_candidate(
            &mut self.codes_candidate,
            &mut self.code_commitments,
            self.threshold,
        );

        let blocks_candidate = Self::process_multisigned_candidate(
            &mut self.blocks_candidate,
            &mut self.block_commitments,
            self.threshold,
        );

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
            self.codes_candidate.as_mut(),
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
            self.blocks_candidate.as_mut(),
        )
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }

    pub fn get_status_receiver(&self) -> watch::Receiver<SequencerStatus> {
        self.status_sender.subscribe()
    }

    pub fn get_candidate_code_commitments(
        &self,
        digest: Digest,
    ) -> Option<impl Iterator<Item = &CodeCommitment> + '_> {
        Self::get_candidate_commitments(digest, &self.codes_candidate, &self.code_commitments)
    }

    pub fn get_candidate_block_commitments(
        &self,
        digest: Digest,
    ) -> Option<impl Iterator<Item = &BlockCommitment> + '_> {
        Self::get_candidate_commitments(digest, &self.blocks_candidate, &self.block_commitments)
    }

    fn get_candidate_commitments<'a, C>(
        digest: Digest,
        candidate: &'a Option<Candidate>,
        commitments: &'a CommitmentsMap<C>,
    ) -> Option<impl Iterator<Item = &'a C> + 'a> {
        let Some((candidate_digest, (digests, _))) = candidate else {
            return None;
        };

        if *candidate_digest != digest {
            return None;
        }

        Some(digests.iter().map(|digest| {
            commitments
                .get(digest)
                .map(|(commitment, _)| commitment)
                .unwrap_or_else(|| {
                    unreachable!("Guarantied by `Sequencer` implementation to be in the map")
                })
        }))
    }

    fn set_candidate_commitments<C: AsDigest>(
        commitments: &CommitmentsMap<C>,
        candidate: &mut Option<Candidate>,
        threshold: u64,
    ) -> Option<Digest> {
        let suitable_digests: Vec<_> = commitments
            .iter()
            .filter_map(|(&digest, (_, set))| (set.len() as u64 >= threshold).then_some(digest))
            .collect();

        if suitable_digests.is_empty() {
            return None;
        }

        let digest = suitable_digests.as_digest();

        *candidate = Some((
            digest,
            (suitable_digests.into_iter().collect(), HashMap::new()),
        ));

        Some(digest)
    }

    fn process_multisigned_candidate<C: AsDigest>(
        candidate: &mut Option<Candidate>,
        commitments: &mut CommitmentsMap<C>,
        threshold: u64,
    ) -> Option<MultisignedCommitments<C>> {
        if candidate
            .as_ref()
            .map(|(_digest, (_, sigs))| threshold > sigs.len() as u64)
            .unwrap_or(true)
        {
            return None;
        }

        let (_, (digests, signatures)) = candidate.take()?;

        if digests.is_empty() {
            unreachable!("Guarantied by `Sequencer` implementation to be not empty");
        }

        let commitments: Vec<_> = digests
            .iter()
            .map(|digest| {
                commitments
                    .remove(digest)
                    .map(|(commitment, _)| commitment)
                    .unwrap_or_else(|| {
                        unreachable!("Guarantied by `Sequencer` implementation to be in the map");
                    })
            })
            .collect();

        let sources = signatures.keys().cloned().collect();
        let signatures = signatures.values().cloned().collect();

        Some(MultisignedCommitments {
            commitments,
            sources,
            signatures,
        })
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
        commitments_storage: &mut BTreeMap<Digest, (C, HashSet<Address>)>,
    ) -> Result<()> {
        if validators.contains(&origin).not() {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if aggregated.recover(router_address)? != origin {
            return Err(anyhow!("Signature verification failed for {origin}"));
        }

        let mut processed = HashSet::new();
        for commitment in aggregated.commitments {
            let digest = commitment.as_digest();
            if !processed.insert(digest) {
                continue;
            }
            let (_, set) = commitments_storage
                .entry(digest)
                .or_insert_with(|| (commitment, HashSet::new()));
            set.insert(origin);
        }

        Ok(())
    }

    fn receive_signature(
        origin: Address,
        digest: Digest,
        signature: Signature,
        validators: &HashSet<Address>,
        router_address: Address,
        candidate: Option<&mut Candidate>,
    ) -> Result<()> {
        let Some((candidate_digest, (_, signatures))) = candidate else {
            return Err(anyhow!("No candidate found"));
        };

        if *candidate_digest != digest {
            return Err(anyhow!("Digest mismatch"));
        }

        if !validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if agro::recover_from_digest(digest, &signature, router_address)? != origin {
            return Err(anyhow!("Invalid signature"));
        }

        signatures.insert(origin, signature);

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
