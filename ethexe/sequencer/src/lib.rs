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

use agro::CommitmentsDigestSigner;
use anyhow::{anyhow, Result};
use ethexe_common::{BlockCommitment, CodeCommitment};
use ethexe_ethereum::Ethereum;
use ethexe_observer::Event;
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommitmentAndOrigins<C> {
    commitment: C,
    origins: BTreeSet<Address>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultisignedCommitments<C> {
    commitments: BTreeMap<Digest, C>,
    signatures: BTreeMap<Address, Signature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultisignedDigests {
    digests: BTreeSet<Digest>,
    signatures: BTreeMap<Address, Signature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    digest: Digest,
    multisigned: MultisignedDigests,
}

type CommitmentsMap<C> = BTreeMap<Digest, CommitmentAndOrigins<C>>;

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
        message.extend_from_slice(self.transitions_digest.as_ref());

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
            code_commitments_len = candidate.commitments.len() as u64;
            log::debug!("Collected {code_commitments_len} code commitments. Trying to submit...");

            codes_future = Some(self.submit_codes_commitment(candidate));
        };

        if let Some(candidate) = blocks_candidate {
            block_commitments_len = candidate.commitments.len() as u64;
            log::debug!("Collected {block_commitments_len} block commitments. Trying to submit...",);

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
        let Some(candidate) = candidate else {
            return None;
        };

        if candidate.digest != digest {
            return None;
        }

        Some(candidate.multisigned.digests.iter().map(|digest| {
            commitments
                .get(digest)
                .map(|c| &c.commitment)
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
            .filter_map(|(digest, c)| (c.origins.len() as u64 >= threshold).then_some(*digest))
            .collect();

        if suitable_digests.is_empty() {
            return None;
        }

        let digest = suitable_digests.as_digest();

        *candidate = Some(Candidate {
            digest,
            multisigned: MultisignedDigests {
                digests: suitable_digests.into_iter().collect(),
                signatures: Default::default(),
            },
        });

        Some(digest)
    }

    fn process_multisigned_candidate<C: AsDigest>(
        candidate: &mut Option<Candidate>,
        commitments: &mut CommitmentsMap<C>,
        threshold: u64,
    ) -> Option<MultisignedCommitments<C>> {
        if candidate
            .as_ref()
            .map(|c| threshold > c.multisigned.signatures.len() as u64)
            .unwrap_or(true)
        {
            return None;
        }

        let Candidate { multisigned, .. } = candidate.take()?;

        if multisigned.digests.is_empty() {
            unreachable!("Guarantied by `Sequencer` implementation to be not empty");
        }

        let commitments = multisigned
            .digests
            .iter()
            .map(|digest| {
                commitments
                    .remove(digest)
                    .map(|c| (*digest, c.commitment))
                    .unwrap_or_else(|| {
                        unreachable!("Guarantied by `Sequencer` implementation to be in the map");
                    })
            })
            .collect();

        Some(MultisignedCommitments {
            commitments,
            signatures: multisigned.signatures,
        })
    }

    async fn submit_codes_commitment(
        &self,
        MultisignedCommitments {
            commitments,
            signatures,
        }: MultisignedCommitments<CodeCommitment>,
    ) -> Result<()> {
        let codes = commitments.into_values().map(Into::into).collect();
        let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

        log::debug!("Code commitments to submit: {codes:?}, signed by: {origins:?}",);

        let router = self.ethereum.router();
        if let Err(e) = router.commit_codes(codes, signatures).await {
            // TODO: return error?
            log::error!("Failed to commit code ids: {e}");
        }

        Ok(())
    }

    async fn submit_block_commitments(
        &self,
        MultisignedCommitments {
            commitments,
            signatures,
        }: MultisignedCommitments<BlockCommitment>,
    ) -> Result<()> {
        let blocks = commitments.into_values().map(Into::into).collect();
        let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

        log::debug!("Block commitments to submit: {blocks:?}, signed by: {origins:?}",);

        let router = self.ethereum.router();
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

    fn receive_commitments<C: AsDigest>(
        origin: Address,
        aggregated: AggregatedCommitments<C>,
        validators: &HashSet<Address>,
        router_address: Address,
        commitments_storage: &mut CommitmentsMap<C>,
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
            commitments_storage
                .entry(digest)
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
        origin: Address,
        commitments_digest: Digest,
        signature: Signature,
        validators: &HashSet<Address>,
        router_address: Address,
        candidate: Option<&mut Candidate>,
    ) -> Result<()> {
        let Some(candidate) = candidate else {
            return Err(anyhow!("No candidate found"));
        };

        if candidate.digest != commitments_digest {
            return Err(anyhow!("Digest mismatch"));
        }

        if !validators.contains(&origin) {
            return Err(anyhow!("Unknown validator {origin}"));
        }

        if Signer::recover_from_commitments_digest(commitments_digest, &signature, router_address)?
            != origin
        {
            return Err(anyhow!("Invalid signature"));
        }

        candidate.multisigned.signatures.insert(origin, signature);

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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_signer::PrivateKey;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestComm([u8; 2]);

    impl AsDigest for TestComm {
        fn as_digest(&self) -> Digest {
            self.0.as_digest()
        }
    }

    #[test]
    fn test_receive_signature() {
        let router_address = Address([1; 20]);

        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators: HashSet<_> = validators_private_keys
            .iter()
            .cloned()
            .map(|key| PublicKey::from(key).to_address())
            .collect();

        let validator1_private_key = validators_private_keys[0];
        let validator1_pub_key = PublicKey::from(validator1_private_key);
        let validator1 = validator1_pub_key.to_address();

        let commitments = [TestComm([0, 1]), TestComm([2, 3])];
        let commitments_digest = commitments.as_digest();
        let aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &validator1_private_key,
            validator1_pub_key,
            router_address,
        )
        .unwrap();
        let signature = aggregated.signature;

        Sequencer::receive_signature(
            validator1,
            commitments_digest,
            signature,
            &validators,
            router_address,
            None,
        )
        .expect_err("No candidate is provided");

        let mut signatures: BTreeMap<_, _> = Default::default();
        let digests: BTreeSet<_> = commitments.iter().map(AsDigest::as_digest).collect();
        let mut candidate = Candidate {
            digest: commitments_digest,
            multisigned: MultisignedDigests {
                digests: digests.clone(),
                signatures: signatures.clone(),
            },
        };

        Sequencer::receive_signature(
            validator1,
            Digest::from([1; 32]),
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .expect_err("Incorrect digest has been provided");

        Sequencer::receive_signature(
            Address([3; 20]),
            commitments_digest,
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .expect_err("Unknown validator has been provided");

        Sequencer::receive_signature(
            validator1,
            commitments_digest,
            Signature::create_for_digest(validator1_private_key, Digest::from([1; 32])).unwrap(),
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .expect_err("Signature verification must fail");

        Sequencer::receive_signature(
            validator1,
            commitments_digest,
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .unwrap();

        signatures.insert(validator1, signature);
        assert_eq!(
            candidate,
            Candidate {
                digest: commitments_digest,
                multisigned: MultisignedDigests {
                    digests: digests.clone(),
                    signatures: signatures.clone(),
                },
            }
        );

        let validator2_private_key = validators_private_keys[1];
        let validator2_pub_key = PublicKey::from(validator2_private_key);
        let validator2 = validator2_pub_key.to_address();

        let aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &validator2_private_key,
            validator2_pub_key,
            router_address,
        )
        .unwrap();
        let signature = aggregated.signature;

        Sequencer::receive_signature(
            validator2,
            commitments_digest,
            signature,
            &validators,
            router_address,
            Some(&mut candidate),
        )
        .unwrap();

        signatures.insert(validator2, signature);
        assert_eq!(
            candidate,
            Candidate {
                digest: commitments_digest,
                multisigned: MultisignedDigests {
                    digests,
                    signatures,
                },
            }
        );
    }

    #[test]
    fn test_receive_commitments() {
        let router_address = Address([1; 20]);

        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators: HashSet<_> = validators_private_keys
            .iter()
            .cloned()
            .map(|key| PublicKey::from(key).to_address())
            .collect();

        let validator1_private_key = validators_private_keys[0];
        let validator1_pub_key = PublicKey::from(validator1_private_key);
        let validator1 = validator1_pub_key.to_address();

        let commitments = [TestComm([0, 1]), TestComm([2, 3])];
        let aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &validator1_private_key,
            validator1_pub_key,
            router_address,
        )
        .unwrap();

        let mut expected_commitments_storage = CommitmentsMap::new();
        let mut commitments_storage = CommitmentsMap::new();

        Sequencer::receive_commitments(
            Address([3; 20]),
            aggregated.clone(),
            &validators,
            router_address,
            &mut commitments_storage,
        )
        .expect_err("Unknown validator has been provided");

        let private_key = PrivateKey([3; 32]);
        let incorrect_aggregated = AggregatedCommitments::aggregate_commitments(
            commitments.to_vec(),
            &private_key,
            private_key.into(),
            router_address,
        )
        .unwrap();
        Sequencer::receive_commitments(
            validator1,
            incorrect_aggregated,
            &validators,
            router_address,
            &mut commitments_storage,
        )
        .expect_err("Signature verification must fail");

        Sequencer::receive_commitments(
            validator1,
            aggregated.clone(),
            &validators,
            router_address,
            &mut commitments_storage,
        )
        .unwrap();

        expected_commitments_storage.insert(
            commitments[0].as_digest(),
            CommitmentAndOrigins {
                commitment: commitments[0],
                origins: [validator1].iter().cloned().collect(),
            },
        );
        expected_commitments_storage.insert(
            commitments[1].as_digest(),
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
            &validator2_private_key,
            validator2_pub_key,
            router_address,
        )
        .unwrap();

        Sequencer::receive_commitments(
            validator2,
            aggregated,
            &validators,
            router_address,
            &mut commitments_storage,
        )
        .unwrap();

        expected_commitments_storage
            .get_mut(&commitments[0].as_digest())
            .unwrap()
            .origins
            .insert(validator2);
        expected_commitments_storage
            .get_mut(&commitments[1].as_digest())
            .unwrap()
            .origins
            .insert(validator2);
        assert_eq!(expected_commitments_storage, commitments_storage);
    }

    #[test]
    fn test_set_candidate_commitments() {
        let mut candidate = None;
        let threshold = 2;

        let mut commitments = BTreeMap::new();
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            None
        );
        assert_eq!(candidate, None);

        let commitment1 = TestComm([0, 1]);
        commitments.insert(
            commitment1.as_digest(),
            CommitmentAndOrigins {
                commitment: commitment1,
                origins: Default::default(),
            },
        );
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            None
        );
        assert_eq!(candidate, None);

        commitments
            .get_mut(&commitment1.as_digest())
            .unwrap()
            .origins
            .insert(Address([1; 20]));
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            None
        );
        assert_eq!(candidate, None);

        commitments
            .get_mut(&commitment1.as_digest())
            .unwrap()
            .origins
            .insert(Address([2; 20]));
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            Some([commitment1].as_digest())
        );
        assert_eq!(
            candidate,
            Some(Candidate {
                digest: [commitment1].as_digest(),
                multisigned: MultisignedDigests {
                    digests: [commitment1.as_digest()].iter().cloned().collect(),
                    signatures: Default::default(),
                },
            })
        );

        let commitment2 = TestComm([2, 3]);
        commitments.insert(
            commitment2.as_digest(),
            CommitmentAndOrigins {
                commitment: commitment2,
                origins: [Address([3; 20]), Address([4; 20])]
                    .iter()
                    .cloned()
                    .collect(),
            },
        );
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            Some([commitment1, commitment2].as_digest())
        );
        assert_eq!(
            candidate,
            Some(Candidate {
                digest: [commitment1, commitment2].as_digest(),
                multisigned: MultisignedDigests {
                    digests: [commitment1.as_digest(), commitment2.as_digest()]
                        .iter()
                        .cloned()
                        .collect(),
                    signatures: Default::default(),
                },
            })
        );

        let commitment3 = TestComm([4, 5]);
        commitments.insert(
            commitment3.as_digest(),
            CommitmentAndOrigins {
                commitment: commitment3,
                origins: [Address([5; 20])].iter().cloned().collect(),
            },
        );
        assert_eq!(
            Sequencer::set_candidate_commitments(&commitments, &mut candidate, threshold),
            Some([commitment1, commitment2].as_digest())
        );
        assert_eq!(
            candidate,
            Some(Candidate {
                digest: [commitment1, commitment2].as_digest(),
                multisigned: MultisignedDigests {
                    digests: [commitment1.as_digest(), commitment2.as_digest()]
                        .iter()
                        .cloned()
                        .collect(),
                    signatures: Default::default(),
                },
            })
        );
    }

    #[test]
    #[should_panic(expected = "Guarantied by `Sequencer` implementation to be in the map")]
    fn test_process_multisigned_candidate_empty_map() {
        let candidate = Candidate {
            digest: [1; 32].into(),
            multisigned: MultisignedDigests {
                digests: [[2; 32]].map(Into::into).into_iter().collect(),
                signatures: Default::default(),
            },
        };
        Sequencer::process_multisigned_candidate::<TestComm>(
            &mut Some(candidate),
            &mut Default::default(),
            0,
        );
    }

    #[test]
    #[should_panic(expected = "Guarantied by `Sequencer` implementation to be not empty")]
    fn test_process_multisigned_candidate_empty_digests() {
        let candidate = Candidate {
            digest: [1; 32].into(),
            multisigned: MultisignedDigests {
                digests: Default::default(),
                signatures: Default::default(),
            },
        };
        Sequencer::process_multisigned_candidate::<TestComm>(
            &mut Some(candidate),
            &mut Default::default(),
            0,
        );
    }

    #[test]
    fn test_process_multisigned_candidate() {
        // Test candidate is None
        assert_eq!(
            Sequencer::process_multisigned_candidate::<TestComm>(
                &mut None,
                &mut Default::default(),
                0
            ),
            None
        );

        // Test not enough signatures
        let mut candidate = Some(Candidate {
            digest: [1; 32].into(),
            multisigned: MultisignedDigests {
                digests: Default::default(),
                signatures: Default::default(),
            },
        });
        assert_eq!(
            Sequencer::process_multisigned_candidate(
                &mut candidate,
                &mut CommitmentsMap::<TestComm>::new(),
                2
            ),
            None
        );

        let mut commitments_map = CommitmentsMap::new();
        let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
        let validators_pub_keys = validators_private_keys.map(PublicKey::from);
        let origins: BTreeSet<_> = validators_pub_keys
            .map(|k| k.to_address())
            .into_iter()
            .collect();

        let commitments = [TestComm([0, 1]), TestComm([2, 3]), TestComm([4, 5])];
        commitments.iter().for_each(|commitment| {
            commitments_map.insert(
                commitment.as_digest(),
                CommitmentAndOrigins {
                    commitment: *commitment,
                    origins: origins.clone(),
                },
            );
        });

        Sequencer::set_candidate_commitments(&commitments_map, &mut candidate, 2).unwrap();

        let mut candidate = candidate.unwrap();
        let router_address = Address([1; 20]);
        let signatures = [0, 1].map(|i| {
            AggregatedCommitments::aggregate_commitments(
                commitments.to_vec(),
                &validators_private_keys[i],
                validators_pub_keys[i],
                router_address,
            )
            .unwrap()
            .signature
        });
        candidate.multisigned.signatures = [0, 1]
            .map(|i| (validators_pub_keys[i].to_address(), signatures[i]))
            .into_iter()
            .collect();

        let mut candidate = Some(candidate);
        assert!(
            Sequencer::process_multisigned_candidate(&mut candidate, &mut commitments_map, 2)
                .is_some(),
        );
        assert!(commitments_map.is_empty());
        assert_eq!(candidate, None);
    }
}
