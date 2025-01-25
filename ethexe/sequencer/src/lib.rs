// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use agro::{AggregatedCommitments, MultisignedCommitmentDigests, MultisignedCommitments};
use anyhow::{anyhow, bail, Result};
use ethexe_common::{
    db::BlockMetaStorage,
    gear::{BlockCommitment, CodeCommitment},
};
use ethexe_ethereum::{router::Router, Ethereum};
use ethexe_service_common::{StreamAlike, Timer};
use ethexe_signer::{Address, Digest, PublicKey, Signature, Signer, ToDigest};
use gprimitives::H256;
use indexmap::IndexSet;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    ops::Not,
    time::Duration,
};

pub mod agro;

#[cfg(test)]
mod tests;

pub type CommitmentsMap<C> = BTreeMap<Digest, CommitmentAndOrigins<C>>;

pub struct SequencerConfig {
    pub ethereum_rpc: String,
    pub sign_tx_public: PublicKey,
    pub router_address: Address,
    pub validators: Vec<Address>,
    pub threshold: u64,
    pub block_time: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SequencerEvent {
    CollectionRoundEnded { block_hash: H256 },
    ValidationRoundEnded { block_hash: H256, submitted: bool },
}

pub struct SequencerService {
    db: Box<dyn BlockMetaStorage>,

    ethereum: Ethereum,
    key: PublicKey,

    threshold: u64,
    validators: HashSet<Address>,
    waiting_for_commitments: BTreeSet<H256>,

    block_commitments: CommitmentsMap<BlockCommitment>,
    code_commitments: CommitmentsMap<CodeCommitment>,

    blocks_candidate: Option<MultisignedCommitmentDigests>,
    codes_candidate: Option<MultisignedCommitmentDigests>,

    status: SequencerStatus,

    // TODO: consider merging into single timer.
    collection_round: Timer<H256>,
    validation_round: Timer<H256>,
}

impl StreamAlike for SequencerService {
    type Item = SequencerEvent;

    async fn like_next(&mut self) -> Option<Self::Item> {
        Some(self.next().await)
    }
}

// TODO: fix it by some wrapper. It's not possible to implement Stream for SequencerService like this.
// impl Stream for SequencerService {
//     type Item = SequencerEvent;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         let e = ready!(pin!(self.next_event()).poll(cx));
//         Poll::Ready(Some(e))
//     }
// }

// impl FusedStream for SequencerService {
//     fn is_terminated(&self) -> bool {
//         false
//     }
// }

impl SequencerService {
    pub async fn new(
        config: &SequencerConfig,
        signer: Signer,
        db: Box<dyn BlockMetaStorage>,
    ) -> Result<Self> {
        Ethereum::new(
            &config.ethereum_rpc,
            config.router_address,
            signer,
            config.sign_tx_public.to_address(),
        )
        .await
        .map(|ethereum| Self {
            db,

            ethereum,
            key: config.sign_tx_public,

            threshold: config.threshold,
            validators: config.validators.iter().cloned().collect(),
            waiting_for_commitments: Default::default(),

            block_commitments: Default::default(),
            code_commitments: Default::default(),

            blocks_candidate: None,
            codes_candidate: None,

            status: Default::default(),

            collection_round: Timer::new("collection", config.block_time / 4),
            validation_round: Timer::new("validation", config.block_time / 4),
        })
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }

    pub fn status(&self) -> SequencerStatus {
        self.status
    }

    pub fn on_new_head(&mut self, hash: H256) -> Result<()> {
        if self
            .db
            .block_end_state_is_valid(hash)
            .is_none_or(|valid| !valid)
        {
            bail!("Block {hash} database state is not valid");
        }

        // TODO: add status
        self.waiting_for_commitments = self
            .db
            .block_commitment_queue(hash)
            .ok_or_else(|| anyhow!("Block {hash} has not block commitment queue"))?
            .into_iter()
            .collect();

        // Remove all commitments, which we are not waiting for anymore.
        self.block_commitments
            .retain(|_, c| self.waiting_for_commitments.contains(&c.commitment.hash));

        self.blocks_candidate.take();
        self.codes_candidate.take();

        log::debug!("[SEQUENCER] Collection round for {hash} started");
        self.collection_round.start(hash);

        if let Some(block) = self.validation_round.stop() {
            log::debug!("[SEQUENCER] Validation round for {block} stopped");
        }

        Ok(())
    }

    pub fn get_candidate_code_commitments(&self) -> impl Iterator<Item = &CodeCommitment> + '_ {
        Self::get_candidate_commitments(&self.codes_candidate, &self.code_commitments)
    }

    pub fn get_candidate_block_commitments(&self) -> impl Iterator<Item = &BlockCommitment> + '_ {
        Self::get_candidate_commitments(&self.blocks_candidate, &self.block_commitments)
    }

    pub fn receive_code_commitments(
        &mut self,
        aggregated: AggregatedCommitments<CodeCommitment>,
    ) -> Result<()> {
        log::debug!("[SEQUENCER] Received code commitments: {aggregated:?}");

        Self::receive_commitments(
            aggregated,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.code_commitments,
            |_| true,
        )
    }

    pub fn receive_block_commitments(
        &mut self,
        aggregated: AggregatedCommitments<BlockCommitment>,
    ) -> Result<()> {
        log::debug!("[SEQUENCER] Received block commitments: {aggregated:?}");

        Self::receive_commitments(
            aggregated,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.block_commitments,
            |c| self.waiting_for_commitments.contains(&c.hash),
        )
    }

    pub fn receive_codes_signature(&mut self, digest: Digest, signature: Signature) -> Result<()> {
        log::debug!("[SEQUENCER] Received codes signature: {digest:?} {signature:?}");

        Self::receive_signature(
            digest,
            signature,
            &self.validators,
            self.ethereum.router().address(),
            self.codes_candidate.as_mut(),
        )
    }

    pub fn receive_blocks_signature(&mut self, digest: Digest, signature: Signature) -> Result<()> {
        log::debug!("[SEQUENCER] Received block signature: {digest:?} {signature:?}");

        Self::receive_signature(
            digest,
            signature,
            &self.validators,
            self.ethereum.router().address(),
            self.blocks_candidate.as_mut(),
        )
    }

    pub async fn submit_multisigned_commitments(&mut self) -> Result<()> {
        let mut codes_future = None;
        let mut blocks_future = None;

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
            let n = candidate.commitments().len();

            log::debug!("Collected {n} code commitments. Submitting...");
            self.status.submitted_code_commitments += n;

            codes_future = Some(Self::submit_codes_commitments(
                self.ethereum.router(),
                candidate,
            ));
        };

        if let Some(candidate) = blocks_candidate {
            let n = candidate.commitments().len();

            log::debug!("Collected {n} block commitments. Submitting...",);
            self.status.submitted_block_commitments += n;

            blocks_future = Some(Self::submit_block_commitments(
                self.ethereum.router(),
                candidate,
            ));
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

        Ok(())
    }

    async fn submit_codes_commitments(
        router: Router,
        multisigned: MultisignedCommitments<CodeCommitment>,
    ) -> Result<()> {
        let (codes, signatures) = multisigned.into_parts();
        let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

        log::debug!("Code commitments to submit: {codes:?}, signed by: {origins:?}",);

        if let Err(e) = router.commit_codes(codes, signatures).await {
            // TODO: return error?
            log::error!("Failed to commit code ids: {e}");
        }

        Ok(())
    }

    async fn submit_block_commitments(
        router: Router,
        multisigned: MultisignedCommitments<BlockCommitment>,
    ) -> Result<()> {
        let (blocks, signatures) = multisigned.into_parts();
        let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

        log::debug!("Block commitments to submit: {blocks:?}, signed by: {origins:?}",);

        match router.commit_blocks(blocks, signatures).await {
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

    pub async fn next(&mut self) -> SequencerEvent {
        tokio::select! {
            block_hash = self.collection_round.rings() => {
                // If chain head is not yet processed by this node, this is normal situation,
                // so we just skip this round for sequencer.
                let Some(block_is_empty) = self.db.block_is_empty(block_hash) else {
                    log::warn!("Failed to get block emptiness status for {block_hash}");
                    return SequencerEvent::CollectionRoundEnded { block_hash };
                };

                let last_non_empty_block = if block_is_empty {
                    let Some(prev_commitment) = self.db.previous_committed_block(block_hash) else {
                        return SequencerEvent::CollectionRoundEnded { block_hash };
                    };

                    prev_commitment
                } else {
                    block_hash
                };

                self.blocks_candidate =
                    Self::blocks_commitment_candidate(&self.block_commitments, last_non_empty_block, self.threshold);
                self.codes_candidate =
                    Self::codes_commitment_candidate(&self.code_commitments, self.threshold);

                let to_start_validation = self.blocks_candidate.is_some() || self.codes_candidate.is_some();

                if to_start_validation {
                    log::debug!("[SEQUENCER] Validation round for {block_hash} started");
                    self.validation_round.start(block_hash);
                }

                SequencerEvent::CollectionRoundEnded { block_hash }
            }
            block_hash = self.validation_round.rings() => {
                log::debug!("Validation round for {block_hash} ended");

                let mut submitted = false;

                if self.blocks_candidate.is_some() || self.codes_candidate.is_some() {
                    log::debug!("Submitting commitments");

                    if let Err(e) = self.submit_multisigned_commitments().await {
                        log::error!("Failed to submit multisigned commitments: {e}");
                    } else {
                        submitted = true;
                    }
                } else {
                    log::debug!("No commitments to submit, skipping");
                }

                SequencerEvent::ValidationRoundEnded { block_hash, submitted }
            }
        }
    }

    fn blocks_commitment_candidate(
        commitments: &CommitmentsMap<BlockCommitment>,
        from_block: H256,
        threshold: u64,
    ) -> Option<MultisignedCommitmentDigests> {
        let suitable_commitments: BTreeMap<_, _> = commitments
            .iter()
            .filter_map(|(digest, c)| {
                (c.origins.len() as u64 >= threshold)
                    .then_some((c.commitment.hash, (digest, &c.commitment)))
            })
            .collect();

        let mut candidate = VecDeque::new();
        let mut block_hash = from_block;
        loop {
            let Some((digest, commitment)) = suitable_commitments.get(&block_hash) else {
                break;
            };

            candidate.push_front(**digest);

            block_hash = commitment.previous_committed_block;
        }

        if candidate.is_empty() {
            return None;
        }

        let candidate = MultisignedCommitmentDigests::new(candidate.into_iter().collect())
            .unwrap_or_else(|err| {
                unreachable!(
                    "Guarantied by impl to be non-empty and without duplicates, but get: {err}"
                );
            });

        Some(candidate)
    }

    fn codes_commitment_candidate(
        commitments: &CommitmentsMap<CodeCommitment>,
        threshold: u64,
    ) -> Option<MultisignedCommitmentDigests> {
        let suitable_commitment_digests: IndexSet<_> = commitments
            .iter()
            .filter_map(|(&digest, c)| (c.origins.len() as u64 >= threshold).then_some(digest))
            .collect();

        if suitable_commitment_digests.is_empty() {
            return None;
        }

        Some(
            MultisignedCommitmentDigests::new(suitable_commitment_digests).unwrap_or_else(|err| {
                unreachable!("Guarantied by impl to be non-empty, but get: {err}");
            }),
        )
    }

    fn get_candidate_commitments<'a, C>(
        candidate: &'a Option<MultisignedCommitmentDigests>,
        commitments: &'a CommitmentsMap<C>,
    ) -> impl Iterator<Item = &'a C> + 'a {
        candidate
            .iter()
            .flat_map(|candidate| candidate.digests().iter())
            .map(|digest| {
                commitments
                    .get(digest)
                    .map(|c| &c.commitment)
                    .unwrap_or_else(|| {
                        unreachable!("Guarantied by `Sequencer` implementation to be in the map")
                    })
            })
    }
    // TODO: make a test that filter works correctly
    fn receive_commitments<C: ToDigest>(
        aggregated: AggregatedCommitments<C>,
        validators: &HashSet<Address>,
        router_address: Address,
        commitments_storage: &mut CommitmentsMap<C>,
        commitments_filter: impl Fn(&C) -> bool,
    ) -> Result<()> {
        let origin = aggregated.recover(router_address)?;

        if validators.contains(&origin).not() {
            return Err(anyhow!("Unknown validator {origin} or invalid signature"));
        }

        for commitment in aggregated.commitments {
            if !commitments_filter(&commitment) {
                continue;
            }

            commitments_storage
                .entry(commitment.to_digest())
                .or_insert_with(|| CommitmentAndOrigins {
                    commitment,
                    origins: Default::default(),
                })
                .origins
                .insert(origin);
        }

        Ok(())
    }

    fn receive_signature(
        commitments_digest: Digest,
        signature: Signature,
        validators: &HashSet<Address>,
        router_address: Address,
        candidate: Option<&mut MultisignedCommitmentDigests>,
    ) -> Result<()> {
        let Some(candidate) = candidate else {
            return Err(anyhow!("No candidate found"));
        };

        candidate.append_signature_with_check(
            commitments_digest,
            signature,
            router_address,
            |origin| {
                validators
                    .contains(&origin)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Unknown validator {origin} or invalid signature"))
            },
        )
    }

    fn process_multisigned_candidate<C: ToDigest>(
        candidate: &mut Option<MultisignedCommitmentDigests>,
        commitments: &mut CommitmentsMap<C>,
        threshold: u64,
    ) -> Option<MultisignedCommitments<C>> {
        if candidate
            .as_ref()
            .map(|c| threshold > c.signatures().len() as u64)
            .unwrap_or(true)
        {
            return None;
        }

        let candidate = candidate.take()?;
        let multisigned = MultisignedCommitments::from_multisigned_digests(candidate, |digest| {
            commitments
                .remove(&digest)
                .map(|c| c.commitment)
                .unwrap_or_else(|| {
                    unreachable!("Guarantied by `Sequencer` implementation to be in the map");
                })
        });

        Some(multisigned)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SequencerStatus {
    pub submitted_code_commitments: usize,
    pub submitted_block_commitments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitmentAndOrigins<C> {
    commitment: C,
    origins: BTreeSet<Address>,
}
