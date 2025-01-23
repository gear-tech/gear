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

use agro::{AggregatedCommitments, MultisignedCommitmentDigests, MultisignedCommitments};
use anyhow::{anyhow, bail, Result};
use ethexe_common::{
    db::BlockMetaStorage,
    gear::{BlockCommitment, CodeCommitment},
    maybe_await,
};
use ethexe_ethereum::{router::Router, Ethereum};
use ethexe_signer::{Address, Digest, PublicKey, Signature, Signer, ToDigest};
use gprimitives::H256;
use indexmap::IndexSet;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    ops::Not,
    time::Duration,
};
use tokio::time::{self, Instant};

pub struct Timer<T = ()> {
    duration: Duration,

    start: Option<Instant>,
    data: Option<T>,
}

impl<T> Timer<T> {
    pub fn new(duration: Duration) -> Self {
        log::debug!("[TIMER] Created timer with duration: {duration:?}");

        Self {
            duration,

            start: None,
            data: None,
        }
    }

    pub fn from_secs(sec: u64) -> Self {
        Self::new(Duration::from_secs(sec))
    }

    pub fn from_millis(millis: u64) -> Self {
        Self::new(Duration::from_millis(millis))
    }

    pub fn started(&self) -> bool {
        self.start.is_some()
    }

    pub fn start(&mut self, data: T) {
        self.data = Some(data);
        let now = Instant::now();
        log::debug!("[TIMER] Started timer at {now:?}");
        self.start = Some(now);
    }

    pub fn stop(&mut self) -> Option<T> {
        self.start = None;
        self.data.take()
    }

    pub async fn rings(&mut self) -> T {
        maybe_await(self.remaining().map(async |dur| {
            log::debug!("[TIMER] Timer will ring in {dur:?}");
            time::sleep(dur).await;
            log::debug!("[TIMER] Timer rings");
            self.stop().expect("must be")
        }))
        .await
    }

    fn remaining(&self) -> Option<Duration> {
        self.start.map(|start| {
            self.duration
                .checked_sub(start.elapsed())
                .unwrap_or(Duration::ZERO)
        })
    }
}

pub struct SequencerServiceConfig {
    pub ethereum_rpc: String,
    pub sign_tx_public: PublicKey,
    pub router_address: Address,
    pub validators: Vec<Address>,
    pub threshold: u64,
    pub block_time: Duration,
}

pub enum SequencerServiceEvent {
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

    // TODO: merge into single timer.
    collection_round: Timer<H256>,
    validation_round: Timer<H256>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitmentAndOrigins<C> {
    commitment: C,
    origins: BTreeSet<Address>,
}

pub type CommitmentsMap<C> = BTreeMap<Digest, CommitmentAndOrigins<C>>;

impl SequencerService {
    pub async fn new(
        config: &SequencerServiceConfig,
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

            collection_round: Timer::new(config.block_time / 2),
            validation_round: Timer::new(config.block_time / 4),
        })
    }

    pub fn address(&self) -> Address {
        self.key.to_address()
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
            log::debug!(
                "Collected {} code commitments. Trying to submit...",
                candidate.commitments().len()
            );
            codes_future = Some(Self::submit_codes_commitments(
                self.ethereum.router(),
                candidate,
            ));
        };

        if let Some(candidate) = blocks_candidate {
            log::debug!(
                "Collected {} block commitments. Trying to submit...",
                candidate.commitments().len()
            );
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

    pub async fn next(&mut self) -> SequencerServiceEvent {
        tokio::select! {
            block_hash = self.collection_round.rings() => {
                // If chain head is not yet processed by this node, this is normal situation,
                // so we just skip this round for sequencer.
                let Some(block_is_empty) = self.db.block_is_empty(block_hash) else {
                    log::warn!("Failed to get block emptiness status for {block_hash}");
                    return SequencerServiceEvent::CollectionRoundEnded { block_hash };
                };

                let last_non_empty_block = if block_is_empty {
                    let Some(prev_commitment) = self.db.previous_committed_block(block_hash) else {
                        return SequencerServiceEvent::CollectionRoundEnded { block_hash };
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

                SequencerServiceEvent::CollectionRoundEnded { block_hash }
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

                SequencerServiceEvent::ValidationRoundEnded { block_hash, submitted }
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Ok;
    use ethexe_signer::{sha3, PrivateKey};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestComm([u8; 2]);

    impl ToDigest for TestComm {
        fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
            sha3::Digest::update(hasher, self.0);
        }
    }

    #[test]
    fn test_receive_signature() {
        let signer = Signer::tmp();

        let router_address = Address([1; 20]);

        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators: HashSet<_> = validators_private_keys
            .iter()
            .cloned()
            .map(|key| signer.add_key(key).unwrap().to_address())
            .collect();

        let validator1_private_key = validators_private_keys[0];
        let validator1_pub_key = PublicKey::from(validator1_private_key);
        let validator1 = validator1_pub_key.to_address();

        let commitments = [TestComm([0, 1]), TestComm([2, 3])];
        let commitments_digest = commitments.to_digest();
        let signature = agro::sign_commitments_digest(
            commitments_digest,
            &signer,
            validator1_pub_key,
            router_address,
        )
        .unwrap();

        SequencerService::receive_signature(
            commitments_digest,
            signature,
            &validators,
            router_address,
            None,
        )
        .expect_err("No candidate is provided");

        let mut signatures: BTreeMap<_, _> = Default::default();
        let digests: IndexSet<_> = commitments.iter().map(ToDigest::to_digest).collect();
        let mut candidate = MultisignedCommitmentDigests::new(digests.clone()).unwrap();

        SequencerService::receive_signature(
            Digest::from([1; 32]),
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .expect_err("Incorrect digest has been provided");

        SequencerService::receive_signature(
            commitments_digest,
            Signature::create_for_digest(validator1_private_key, Digest::from([1; 32])).unwrap(),
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .expect_err("Signature verification must fail");

        SequencerService::receive_signature(
            commitments_digest,
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .unwrap();

        signatures.insert(validator1, signature);
        assert_eq!(candidate.digests(), &digests);
        assert_eq!(candidate.signatures(), &signatures);

        let validator2_private_key = validators_private_keys[1];
        let validator2_pub_key = PublicKey::from(validator2_private_key);
        let validator2 = validator2_pub_key.to_address();

        let signature = agro::sign_commitments_digest(
            commitments_digest,
            &signer,
            validator2_pub_key,
            router_address,
        )
        .unwrap();

        SequencerService::receive_signature(
            commitments_digest,
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .unwrap();

        signatures.insert(validator2, signature);
        assert_eq!(candidate.digests(), &digests);
        assert_eq!(candidate.signatures(), &signatures);
    }

    #[test]
    fn test_receive_commitments() {
        let signer = Signer::tmp();

        let router_address = Address([1; 20]);

        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators: HashSet<_> = validators_private_keys
            .iter()
            .cloned()
            .map(|key| signer.add_key(key).unwrap().to_address())
            .collect();

        let validator1_private_key = validators_private_keys[0];
        let validator1_pub_key = PublicKey::from(validator1_private_key);
        let validator1 = validator1_pub_key.to_address();

        let commitments = [TestComm([0, 1]), TestComm([2, 3])];
        let aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &signer,
            validator1_pub_key,
            router_address,
        )
        .unwrap();

        let mut expected_commitments_storage = CommitmentsMap::new();
        let mut commitments_storage = CommitmentsMap::new();

        let private_key = PrivateKey([3; 32]);
        let pub_key = signer.add_key(private_key).unwrap();
        let incorrect_aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &signer,
            pub_key,
            router_address,
        )
        .unwrap();
        SequencerService::receive_commitments(
            incorrect_aggregated,
            &validators,
            router_address,
            &mut commitments_storage,
            |_| true,
        )
        .expect_err("Signature verification must fail");

        SequencerService::receive_commitments(
            aggregated.clone(),
            &validators,
            router_address,
            &mut commitments_storage,
            |_| true,
        )
        .unwrap();

        expected_commitments_storage.insert(
            commitments[0].to_digest(),
            CommitmentAndOrigins {
                commitment: commitments[0],
                origins: [validator1].iter().cloned().collect(),
            },
        );
        expected_commitments_storage.insert(
            commitments[1].to_digest(),
            CommitmentAndOrigins {
                commitment: commitments[1],
                origins: [validator1].iter().cloned().collect(),
            },
        );
        assert_eq!(expected_commitments_storage, commitments_storage);

        let validator2_private_key = validators_private_keys[1];
        let validator2_pub_key = PublicKey::from(validator2_private_key);
        let validator2 = validator2_pub_key.to_address();

        let aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &signer,
            validator2_pub_key,
            router_address,
        )
        .unwrap();

        SequencerService::receive_commitments(
            aggregated,
            &validators,
            router_address,
            &mut commitments_storage,
            |_| true,
        )
        .unwrap();

        expected_commitments_storage
            .get_mut(&commitments[0].to_digest())
            .unwrap()
            .origins
            .insert(validator2);
        expected_commitments_storage
            .get_mut(&commitments[1].to_digest())
            .unwrap()
            .origins
            .insert(validator2);
        assert_eq!(expected_commitments_storage, commitments_storage);
    }

    #[test]
    fn test_blocks_commitment_candidate() {
        let threshold = 2;

        let mut commitments = BTreeMap::new();

        let commitment1 = BlockCommitment {
            hash: H256::random(),
            timestamp: rand::random(),
            previous_committed_block: H256::random(),
            predecessor_block: H256::random(),
            transitions: Default::default(),
        };
        let commitment2 = BlockCommitment {
            hash: H256::random(),
            timestamp: rand::random(),
            previous_committed_block: commitment1.hash,
            predecessor_block: H256::random(),
            transitions: Default::default(),
        };
        let commitment3 = BlockCommitment {
            hash: H256::random(),
            timestamp: rand::random(),
            previous_committed_block: commitment1.hash,
            predecessor_block: H256::random(),
            transitions: Default::default(),
        };

        let mut expected_digests = IndexSet::new();

        let candidate = SequencerService::blocks_commitment_candidate(
            &commitments,
            commitment1.hash,
            threshold,
        );
        assert!(candidate.is_none());

        commitments.insert(
            commitment1.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment1.clone(),
                origins: Default::default(),
            },
        );
        let candidate =
            SequencerService::blocks_commitment_candidate(&commitments, H256::random(), threshold);
        assert!(candidate.is_none());

        let candidate =
            SequencerService::blocks_commitment_candidate(&commitments, commitment1.hash, 0)
                .expect("Must have candidate");
        expected_digests.insert(commitment1.to_digest());
        assert_eq!(candidate.digests(), &expected_digests);

        commitments
            .get_mut(&commitment1.to_digest())
            .unwrap()
            .origins
            .extend([Address([1; 20]), Address([2; 20])]);
        commitments.insert(
            commitment2.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment2.clone(),
                origins: [[1; 20], [2; 20]].map(Address).iter().cloned().collect(),
            },
        );
        commitments.insert(
            commitment3.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment3.clone(),
                origins: [[1; 20], [2; 20]].map(Address).iter().cloned().collect(),
            },
        );

        let candidate = SequencerService::blocks_commitment_candidate(
            &commitments,
            commitment1.hash,
            threshold,
        )
        .expect("Must have candidate");
        assert_eq!(candidate.digests(), &expected_digests);

        let candidate = SequencerService::blocks_commitment_candidate(
            &commitments,
            commitment2.hash,
            threshold,
        )
        .expect("Must have candidate");
        expected_digests.insert(commitment2.to_digest());
        assert_eq!(candidate.digests(), &expected_digests);

        let candidate = SequencerService::blocks_commitment_candidate(
            &commitments,
            commitment3.hash,
            threshold,
        )
        .expect("Must have candidate");
        expected_digests.pop();
        expected_digests.insert(commitment3.to_digest());
        assert_eq!(candidate.digests(), &expected_digests);
    }

    #[test]
    fn test_codes_commitment_candidate() {
        let threshold = 2;

        let mut commitments = BTreeMap::new();

        let commitment1 = CodeCommitment {
            id: H256::random().0.into(),
            valid: true,
        };
        let commitment2 = CodeCommitment {
            id: H256::random().0.into(),
            valid: true,
        };
        let commitment3 = CodeCommitment {
            id: H256::random().0.into(),
            valid: false,
        };

        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold);
        assert!(candidate.is_none());

        commitments.insert(
            commitment1.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment1.clone(),
                origins: Default::default(),
            },
        );
        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold);
        assert!(candidate.is_none());

        commitments
            .get_mut(&commitment1.to_digest())
            .unwrap()
            .origins
            .insert(Address([1; 20]));
        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold);
        assert!(candidate.is_none());

        commitments
            .get_mut(&commitment1.to_digest())
            .unwrap()
            .origins
            .insert(Address([2; 20]));
        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold)
            .expect("Must have candidate");
        let expected_digests: IndexSet<_> = [commitment1.to_digest()].into_iter().collect();
        assert_eq!(candidate.digests(), &expected_digests);
        assert!(candidate.signatures().is_empty());

        commitments.insert(
            commitment2.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment2.clone(),
                origins: [Address([3; 20]), Address([4; 20])]
                    .iter()
                    .cloned()
                    .collect(),
            },
        );
        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold)
            .expect("Must have candidate");
        let mut expected_digests: IndexSet<_> = [commitment1.to_digest(), commitment2.to_digest()]
            .into_iter()
            .collect();
        expected_digests.sort();
        assert_eq!(candidate.digests(), &expected_digests);
        assert!(candidate.signatures().is_empty());

        commitments.insert(
            commitment3.to_digest(),
            CommitmentAndOrigins {
                commitment: commitment3,
                origins: [Address([5; 20])].iter().cloned().collect(),
            },
        );
        let candidate = SequencerService::codes_commitment_candidate(&commitments, threshold)
            .expect("Must have candidate");
        assert_eq!(candidate.digests(), &expected_digests);
        assert!(candidate.signatures().is_empty());
    }

    #[test]
    #[should_panic(expected = "Guarantied by `Sequencer` implementation to be in the map")]
    fn test_process_multisigned_candidate_empty_map() {
        let candidate =
            MultisignedCommitmentDigests::new([[2; 32]].map(Into::into).into_iter().collect())
                .unwrap();
        SequencerService::process_multisigned_candidate::<TestComm>(
            &mut Some(candidate),
            &mut Default::default(),
            0,
        );
    }

    #[test]
    fn test_process_multisigned_candidate() {
        let signer = Signer::tmp();

        // Test candidate is None
        assert!(SequencerService::process_multisigned_candidate::<TestComm>(
            &mut None,
            &mut Default::default(),
            0
        )
        .is_none());

        // Test not enough signatures
        let mut candidate = Some(
            MultisignedCommitmentDigests::new([b"gear".to_digest()].into_iter().collect()).unwrap(),
        );
        assert!(SequencerService::process_multisigned_candidate(
            &mut candidate,
            &mut CommitmentsMap::<TestComm>::new(),
            2
        )
        .is_none());

        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators_pub_keys = validators_private_keys.map(|key| signer.add_key(key).unwrap());
        let origins: BTreeSet<_> = validators_pub_keys
            .map(|k| k.to_address())
            .into_iter()
            .collect();

        let commitments = [TestComm([0, 1]), TestComm([2, 3]), TestComm([4, 5])];
        let mut commitments_map = commitments
            .iter()
            .map(|commitment| {
                (
                    commitment.to_digest(),
                    CommitmentAndOrigins {
                        commitment: *commitment,
                        origins: origins.clone(),
                    },
                )
            })
            .collect();

        let mut candidate =
            MultisignedCommitmentDigests::new(commitments.iter().map(|c| c.to_digest()).collect())
                .unwrap();

        let router_address = Address([1; 20]);
        validators_pub_keys.iter().for_each(|pub_key| {
            let commitments_digest = commitments.to_digest();
            candidate
                .append_signature_with_check(
                    commitments_digest,
                    agro::sign_commitments_digest(
                        commitments_digest,
                        &signer,
                        *pub_key,
                        router_address,
                    )
                    .unwrap(),
                    router_address,
                    |_| Ok(()),
                )
                .unwrap();
        });

        let mut candidate = Some(candidate);

        assert!(SequencerService::process_multisigned_candidate(
            &mut candidate,
            &mut commitments_map,
            2
        )
        .is_some(),);
        assert!(commitments_map.is_empty());
        assert!(candidate.is_none());
    }
}
