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

use agro::{AggregatedCommitments, MultisignedCommitmentDigests, Signatures};
use anyhow::{anyhow, bail, Result};
use ethexe_common::{
    db::BlockMetaStorage,
    gear::{BatchCommitment, BlockCommitment, CodeCommitment},
};
use ethexe_ethereum::{router::Router, Ethereum};
use ethexe_service_utils::Timer;
use ethexe_signer::{Address, Digest, PublicKey, Signature, Signer, ToDigest};
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::H256;
use indexmap::IndexSet;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    iter,
    ops::Not,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub mod agro;
pub mod bp;
mod producer;
mod verifier;
mod participant;
mod utils;

#[cfg(test)]
mod tests;

pub type CommitmentsMap<C> = BTreeMap<Digest, CommitmentAndOrigins<C>>;

type CommitmentSubmitFuture = BoxFuture<'static, Result<H256>>;

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
    CommitmentSubmitted { tx_hash: Option<H256> },
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

    codes_candidate: Option<MultisignedCommitmentDigests>,
    blocks_candidate: Option<MultisignedCommitmentDigests>,

    signatures: Signatures,

    status: SequencerStatus,

    // TODO: consider merging into single timer.
    collection_round: Timer<H256>,
    validation_round: Timer<H256>,

    submissions: FuturesUnordered<CommitmentSubmitFuture>,
}

impl Stream for SequencerService {
    type Item = SequencerEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(block_hash) = self.collection_round.poll_unpin(cx) {
            let event = self.handle_collection_round_end(block_hash);

            return Poll::Ready(Some(event));
        }

        if let Poll::Ready(block_hash) = self.validation_round.poll_unpin(cx) {
            let event = self.handle_validation_round_end(block_hash);

            return Poll::Ready(Some(event));
        }

        if let Poll::Ready(Some(res)) = self.submissions.poll_next_unpin(cx) {
            let tx_hash = res
                .inspect(|tx_hash| {
                    log::debug!("Successfully submitted batch commitment in tx {tx_hash}")
                })
                .inspect_err(|err| log::warn!("Failed to submit batch commitment: {err}"))
                .ok();

            let event = SequencerEvent::CommitmentSubmitted { tx_hash };

            return Poll::Ready(Some(event));
        }

        Poll::Pending
    }
}

impl FusedStream for SequencerService {
    fn is_terminated(&self) -> bool {
        false
    }
}

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

            codes_candidate: None,
            blocks_candidate: None,

            signatures: Default::default(),

            status: Default::default(),

            collection_round: Timer::new("collection", config.block_time / 4),
            validation_round: Timer::new("validation", config.block_time / 4),

            submissions: FuturesUnordered::new(),
        })
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
    }

    pub fn status(&self) -> SequencerStatus {
        self.status
    }

    pub fn on_new_head(&mut self, hash: H256) -> Result<()> {
        if !self.db.block_computed(hash) {
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

        self.codes_candidate.take();
        self.blocks_candidate.take();

        self.signatures = Default::default();

        log::debug!("Collection round for {hash} started");
        self.collection_round.start(hash);

        if let Some(block) = self.validation_round.stop() {
            log::debug!("Validation round for {block} stopped");
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
        log::debug!("Received code commitments: {aggregated:?}");

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
        log::debug!("Received block commitments: {aggregated:?}");

        Self::receive_commitments(
            aggregated,
            &self.validators,
            self.ethereum.router().address(),
            &mut self.block_commitments,
            |c| self.waiting_for_commitments.contains(&c.hash),
        )
    }

    pub fn receive_batch_commitment_signature(
        &mut self,
        digest: Digest,
        signature: Signature,
    ) -> Result<()> {
        log::debug!("Received batch commitment signature: {digest:?} {signature:?}");

        Self::receive_signature(
            digest,
            signature,
            &self.validators,
            self.ethereum.router().address(),
            &[
                self.codes_candidate.as_ref(),
                self.blocks_candidate.as_ref(),
            ],
            &mut self.signatures,
        )
    }

    pub fn submit_multisigned_commitments(&mut self) {
        let code_commitments = Self::process_multisigned_candidate(
            &mut self.codes_candidate,
            &mut self.code_commitments,
        );
        let code_commitments_len = code_commitments.len();

        let block_commitments = Self::process_multisigned_candidate(
            &mut self.blocks_candidate,
            &mut self.block_commitments,
        );
        let block_commitments_len = block_commitments.len();

        self.status.submitted_code_commitments += code_commitments_len;
        self.status.submitted_block_commitments += block_commitments_len;

        log::debug!("Collected {code_commitments_len} code commitments, {block_commitments_len} block commitments. Submitting...");

        self.submissions.push(Box::pin(
            Self::submit_batch_commitment(
                self.ethereum.router(),
                BatchCommitment {
                    code_commitments,
                    block_commitments,
                },
                self.signatures.clone(),
            )
            .map(|tx_hash| tx_hash),
        ));
    }

    async fn submit_batch_commitment(
        router: Router,
        commitment: BatchCommitment,
        signatures: Signatures,
    ) -> Result<H256> {
        let (origins, signatures): (Vec<_>, Vec<_>) = signatures.into_inner().into_iter().unzip();

        log::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

        router.commit_batch(commitment, signatures).await
    }

    fn handle_collection_round_end(&mut self, block_hash: H256) -> SequencerEvent {
        // If chain head is not yet processed by this node, this is normal situation,
        // so we just skip this round for sequencer.
        let Some(block_is_empty) = self.db.block_outcome_is_empty(block_hash) else {
            log::error!("Failed to get block emptiness status for {block_hash}");
            return SequencerEvent::CollectionRoundEnded { block_hash };
        };

        let last_non_empty_block = if block_is_empty {
            let Some(prev_commitment) = self.db.previous_not_empty_block(block_hash) else {
                return SequencerEvent::CollectionRoundEnded { block_hash };
            };

            prev_commitment
        } else {
            block_hash
        };

        let Some(waiting_for_codes) = self.db.block_codes_queue(block_hash) else {
            log::error!("Failed to get block codes queue for {block_hash}");
            return SequencerEvent::CollectionRoundEnded { block_hash };
        };
        let waiting_for_codes: BTreeSet<_> = waiting_for_codes.into_iter().collect();
        self.codes_candidate = Self::codes_commitment_candidate(
            self.code_commitments
                .iter()
                .filter(|(_, c)| waiting_for_codes.contains(&c.commitment.id)),
            self.threshold,
        );

        self.blocks_candidate = Self::blocks_commitment_candidate(
            &self.block_commitments,
            last_non_empty_block,
            self.threshold,
        );

        self.signatures = Default::default();

        let to_start_validation = self.codes_candidate.is_some() || self.blocks_candidate.is_some();

        if to_start_validation {
            log::debug!("Validation round for {block_hash} started");
            self.validation_round.start(block_hash);
        }

        SequencerEvent::CollectionRoundEnded { block_hash }
    }

    fn handle_validation_round_end(&mut self, block_hash: H256) -> SequencerEvent {
        log::debug!("Validation round for {block_hash} ended");

        let mut submitted = false;

        if (self.codes_candidate.is_some() || self.blocks_candidate.is_some())
            && self.signatures.len() as u64 >= self.threshold
        {
            log::debug!("Submitting commitments");
            self.submit_multisigned_commitments();
            submitted = true;
        } else {
            log::debug!("No commitments to submit, skipping");
        }

        log::debug!("Validation round ended: block {block_hash}, submitted: {submitted}");

        SequencerEvent::ValidationRoundEnded {
            block_hash,
            submitted,
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

    fn codes_commitment_candidate<'a>(
        commitments: impl Iterator<Item = (&'a Digest, &'a CommitmentAndOrigins<CodeCommitment>)> + 'a,
        threshold: u64,
    ) -> Option<MultisignedCommitmentDigests> {
        let suitable_commitment_digests: IndexSet<_> = commitments
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
        candidates: &[Option<&MultisignedCommitmentDigests>],
        signatures: &mut Signatures,
    ) -> Result<()> {
        let candidate_digests = candidates.iter().map(|candidate| match candidate {
            Some(candidate) => candidate.digest(),
            None => iter::empty::<Digest>().collect(),
        });
        let candidate_digest = candidate_digests.collect();

        if commitments_digest != candidate_digest {
            return Err(anyhow!("Aggregated commitments digest mismatch"));
        }

        signatures.append_signature_with_check(
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
    ) -> Vec<C> {
        let Some(candidate) = candidate.take() else {
            return Vec::new();
        };
        candidate
            .into_digests()
            .into_iter()
            .map(|digest| {
                commitments
                    .remove(&digest)
                    .map(|c| c.commitment)
                    .unwrap_or_else(|| {
                        unreachable!("Guarantied by `Sequencer` implementation to be in the map");
                    })
            })
            .collect()
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
