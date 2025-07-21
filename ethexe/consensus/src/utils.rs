// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! # Utilities Module
//!
//! This module provides utility functions and data structures for handling batch commitments,
//! validation requests, and multi-signature operations in the Ethexe system.

use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, AnnounceHash, AnnounceStorageRead, Digest, ProducerBlock, SimpleBlockData, ToDigest,
    db::{BlockMetaStorageRead, CodesStorageRead},
    ecdsa::{ContractSignature, PublicKey, SignedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, GearBlock},
    sha3::{self, digest::Update},
};
use ethexe_signer::Signer;
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, HashSet},
    hash::Hash,
};

pub type SignedProducerBlock = SignedData<ProducerBlock>;
pub type SignedValidationRequest = SignedData<BatchCommitmentValidationRequest>;

/// Represents a request for validating a batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationRequest {
    // Digest of batch commitment to validate
    pub digest: Digest,
    /// List of blocks to validate
    pub head_announce: Option<AnnounceHash>,
    /// List of codes which are part of the batch
    pub codes: Vec<CodeId>,
}

impl BatchCommitmentValidationRequest {
    pub fn new(batch: &BatchCommitment) -> Self {
        let codes = batch
            .code_commitments
            .iter()
            .map(|commitment| commitment.id)
            .collect();

        BatchCommitmentValidationRequest {
            digest: batch.to_digest(),
            head_announce: batch.chain_commitment.map(|cc| cc.head_announce),
            codes,
        }
    }
}

impl ToDigest for BatchCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            digest,
            head_announce,
            codes,
        } = self;

        hasher.update(digest);
        head_announce.map(|ha| hasher.update(ha.0));
        hasher.update(
            codes
                .iter()
                .flat_map(|h| h.into_bytes())
                .collect::<Vec<u8>>(),
        );
    }
}

/// A reply to a batch commitment validation request.
/// Contains the digest of the batch and a signature confirming the validation.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationReply {
    /// Digest of the [`BatchCommitment`] being validated
    pub digest: Digest,
    /// Signature confirming the validation by origin
    pub signature: ContractSignature,
}

/// A batch commitment, that has been signed by multiple validators.
/// This structure manages the collection of signatures from different validators
/// for a single batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct MultisignedBatchCommitment {
    batch: BatchCommitment,
    batch_digest: Digest,
    router_address: Address,
    signatures: BTreeMap<Address, ContractSignature>,
}

impl MultisignedBatchCommitment {
    /// Creates a new multisigned batch commitment with an initial signature.
    ///
    /// # Arguments
    /// * `batch` - The batch commitment to be signed
    /// * `signer` - The contract signer used to create signatures
    /// * `pub_key` - The public key of the initial signer
    ///
    /// # Returns
    /// A new `MultisignedBatchCommitment` instance with the initial signature
    pub fn new(
        batch: BatchCommitment,
        signer: &Signer,
        router_address: Address,
        pub_key: PublicKey,
    ) -> Result<Self> {
        let batch_digest = batch.to_digest();
        let signature = signer.sign_for_contract(router_address, pub_key, batch_digest)?;
        let signatures: BTreeMap<_, _> = [(pub_key.to_address(), signature)].into_iter().collect();

        Ok(Self {
            batch,
            batch_digest,
            router_address,
            signatures,
        })
    }

    /// Accepts a validation reply from another validator and adds it's signature.
    ///
    /// # Arguments
    /// * `reply` - The validation reply containing the signature
    /// * `check_origin` - A closure to verify the origin of the signature
    ///
    /// # Returns
    /// Result indicating success or failure of the operation
    pub fn accept_batch_commitment_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
        check_origin: impl FnOnce(Address) -> Result<()>,
    ) -> Result<()> {
        let BatchCommitmentValidationReply { digest, signature } = reply;

        anyhow::ensure!(digest == self.batch_digest, "Invalid reply digest");

        let origin = signature
            .validate(self.router_address, digest)?
            .to_address();

        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    /// Returns a reference to the map of validator addresses to their signatures
    pub fn signatures(&self) -> &BTreeMap<Address, ContractSignature> {
        &self.signatures
    }

    /// Returns a reference to the underlying batch commitment
    pub fn batch(&self) -> &BatchCommitment {
        &self.batch
    }

    /// Consumes the structure and returns its parts
    ///
    /// # Returns
    /// A tuple containing the batch commitment and the map of signatures
    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, ContractSignature>) {
        (self.batch, self.signatures)
    }
}

pub fn aggregate_code_commitments<DB: CodesStorageRead>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => {
                return Err(anyhow::anyhow!("Code status not found in db: {id}"));
            }
            None => {}
        }
    }

    Ok(commitments)
}

pub fn aggregate_chain_commitment<DB: AnnounceStorageRead + BlockMetaStorageRead>(
    db: &DB,
    head_announce_hash: AnnounceHash,
    fail_if_not_computed: bool,
) -> Result<Option<ChainCommitment>> {
    let head_announce = db
        .announce(head_announce_hash)
        .ok_or_else(|| anyhow!("Cannot get from db announce by hash {head_announce_hash:?}"))?;
    let last_committed_announce_hash = db
        .block_meta(head_announce.block_hash)
        .last_committed_announce
        .ok_or_else(|| {
            anyhow!("Cannot get from db last committed announce for block {head_announce_hash:?}")
        })?;

    let mut chain_commitments = Vec::new();
    let mut announce_hash = head_announce_hash;
    while announce_hash != last_committed_announce_hash {
        if !db.announce_meta(announce_hash).computed {
            // This can happen when validator syncs from p2p network and skips some old blocks.
            if fail_if_not_computed {
                return Err(anyhow!("Block {block} is not computed"));
            } else {
                return Ok(None);
            }
        }

        let transitions = db
            .block_outcome(block)
            .ok_or_else(|| anyhow!("Cannot get from db outcome for computed block {block}"))?;

        let gear_blocks = vec![GearBlock {
            hash: block,
            off_chain_transactions_hash: H256::zero(),
            gas_allowance: 0,
        }];

        chain_commitments.push(ChainCommitment {
            transitions,
            head_announce: gear_blocks,
        });
    }

    Ok(squash_chain_commitments(chain_commitments))
}

pub fn create_batch_commitment<DB: BlockMetaStorageRead>(
    db: &DB,
    block: &SimpleBlockData,
    chain_commitment: Option<ChainCommitment>,
    code_commitments: Vec<CodeCommitment>,
) -> Result<Option<BatchCommitment>> {
    if chain_commitment.is_none() && code_commitments.is_empty() {
        return Ok(None);
    }

    let last_committed = db.last_committed_batch(block.hash).ok_or_else(|| {
        anyhow!(
            "Cannot get from db last committed block for block {}",
            block.hash
        )
    })?;

    Ok(Some(BatchCommitment {
        block_hash: block.hash,
        timestamp: block.header.timestamp,
        previous_batch: last_committed,
        chain_commitment,
        code_commitments,
        validators_commitment: None,
        rewards_commitment: None,
    }))
}

// TODO #4744: improve squashing - removing redundant state transitions
pub fn squash_chain_commitments(
    chain_commitments: Vec<ChainCommitment>,
) -> Option<ChainCommitment> {
    if chain_commitments.is_empty() {
        return None;
    }

    let mut transitions = Vec::new();
    let mut gear_blocks = Vec::new();

    for commitment in chain_commitments {
        transitions.extend(commitment.transitions);
        gear_blocks.extend(commitment.head_announce);
    }

    Some(ChainCommitment {
        transitions,
        head_announce: gear_blocks,
    })
}

pub fn has_duplicates<T: Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

// pub fn choose_announces_chain_head<
//     DB: BlockMetaStorageRead + OnChainStorageRead + AnnounceStorage,
// >(
//     db: DB,
//     head: &SimpleBlockData,
//     genesis: &SimpleBlockData,
// ) -> Result<()> {
//     // Must be guarantied:
//     // 1) `genesis` is from the same branch as `head`
//     // 2) `head.number` > `genesis.number`

//     const MAX_COMMITMENT_DELAY: u32 = 3;

//     let mut counter = 0;
//     let mut block = head.clone();
//     let mut last_committed_announce = None;
//     let mut announces_queue = VecDeque::new();
//     while counter < MAX_COMMITMENT_DELAY && block.hash != genesis.hash {
//         if let Some(announce) = db.block_meta(block.hash).producer_announce {
//             announces_queue.push_back(announce);
//         }

//         if let Some(announce) = db
//             .block_events(block.hash)
//             .ok_or_else(|| {
//                 anyhow::anyhow!("Cannot get from db events for synced block {}", block.hash)
//             })?
//             .into_iter()
//             .filter_map(|event| {
//                 if let BlockEvent::Router(RouterEvent::GearBlockCommitted(announce)) = event {
//                     announces_queue.retain(|a| a != &announce.hash());
//                     Some(announce.hash())
//                 } else {
//                     None
//                 }
//             })
//             .last()
//         {
//             let _ = last_committed_announce.get_or_insert(announce);
//         }

//         block = SimpleBlockData {
//             hash: block.header.parent_hash,
//             header: db.block_header(block.header.parent_hash).ok_or_else(|| {
//                 anyhow::anyhow!("Cannot get from db header for block {}", block.hash)
//             })?,
//         };

//         counter += 1;
//     }

//     let (last_defined_block, mut last_announce) =
//         if let Some(last_committed_announce) = last_committed_announce {
//             let Some(announce) = db.announce_block(last_committed_announce) else {
//                 log::warn!("Cannot get from db announce by hash {last_committed_announce:?}");
//                 todo!();
//             };
//             let last_committed_announce_block_height = db
//                 .block_header(announce.block_hash)
//                 .ok_or_else(|| {
//                     anyhow::anyhow!(
//                         "Cannot get from db header for block {}",
//                         announce.block_hash
//                     )
//                 })?
//                 .height;

//             debug_assert!(last_committed_announce_block_height > genesis.header.height);

//             if last_committed_announce_block_height > block.header.height {
//                 (announce.block_hash, Some(last_committed_announce))
//             } else {
//                 (block.hash, Some(last_committed_announce))
//             }
//         } else {
//             (block.hash, None)
//         };

//     let announces_head = if let Some(announce) = last_announce {
//         announce
//     } else {
//         // Committed announce not found - we have to search for last announce (maybe empty) from which we could propagate the chain.
//         while block.hash != genesis.hash {
//             if let Some(announce) = db
//                 .block_events(block.hash)
//                 .ok_or_else(|| {
//                     anyhow::anyhow!("Cannot get from db events for synced block {}", block.hash)
//                 })?
//                 .into_iter()
//                 .filter_map(|event| {
//                     if let BlockEvent::Router(RouterEvent::GearBlockCommitted(announce)) = event {
//                         Some(announce.hash())
//                     } else {
//                         None
//                     }
//                 })
//                 .last()
//             {
//                 last_announce = Some(announce);
//                 break;
//             }

//             block = SimpleBlockData {
//                 hash: block.header.parent_hash,
//                 header: db.block_header(block.header.parent_hash).ok_or_else(|| {
//                     anyhow::anyhow!("Cannot get from db header for block {}", block.hash)
//                 })?,
//             };
//         }

//         // If we still don't have any committed announces after searching through the chain,
//         // then we use genesis announce hash - zero hash.
//         last_announce.unwrap_or(AnnounceHash(H256::zero()))
//     };

//     // Propagate empty announces from `announce_head` till the `last_defined_block`.
//     let mut last_announce_hash = announces_head;
//     let Some(mut announce) = db.announce_block(announces_head) else {
//         todo!()
//     };
//     let mut block_hash = announce.block_hash;
//     let mut chain = VecDeque::new();
//     while block_hash != last_defined_block {
//         let block = SimpleBlockData {
//             hash: block_hash,
//             header: db.block_header(block_hash).ok_or_else(|| {
//                 anyhow::anyhow!("Cannot get from db header for block {}", block_hash)
//             })?,
//         };
//         block_hash = block.header.parent_hash;
//         chain.push_front(block);
//     }
//     for block in chain {
//         last_announce_hash = ProducerBlock {
//             block_hash: block.hash,
//             parent_announce: last_announce_hash,
//             gas_allowance: None,
//             off_chain_transactions: Vec::new(),
//         }
//         .hash();
//     }

//     // Now we have `last_announce_hash`, which is the announce for the `last_defined_block`.
//     // If have some fresh announces from previous block producers, which are not committed yet,
//     // Then we start to iterate over them, in order to find the first announce,
//     // which is in the same branch as `last_announce_hash`.
//     for announce_hash in announces_queue {
//         let Some(announce) = db.announce_block(announce_hash) else {
//             log::warn!("Cannot get from db announce by hash {announce_hash:?}");
//             continue;
//         };

//         for announce

//         if announce.block_hash == last_defined_block {
//             // We found the announce for the last defined block.
//             // We can use it as a chain head.
//             db.set_announces_chain_head(announce.block_hash, announces_head)?;
//             return Ok(());
//         }

//         if announce.block_hash == announces_head.0 {
//             // We found the announce for the announces head.
//             // We can use it as a chain head.
//             db.set_announces_chain_head(announce.block_hash, announces_head)?;
//             return Ok(());
//         }
//     }

//     Ok(())
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use ethexe_common::{
        db::{BlockMetaStorageWrite, CodesStorageWrite},
        gear::StateTransition,
    };
    use ethexe_db::Database;

    const ADDRESS: Address = Address([42; 20]);

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let multisigned_batch =
            MultisignedBatchCommitment::new(batch.clone(), &signer, ADDRESS, pub_key)
                .expect("Failed to create multisigned batch commitment");

        assert_eq!(multisigned_batch.batch, batch);
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn accept_batch_commitment_validation_reply() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        multisigned_batch
            .accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Attempt to add the same reply again
        multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        // Ensure the number of signatures has not increased
        assert_eq!(multisigned_batch.signatures.len(), 2);
    }

    #[test]
    fn reject_validation_reply_with_incorrect_digest() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let incorrect_digest = [1, 2, 3].to_digest();
        let reply = BatchCommitmentValidationReply {
            digest: incorrect_digest,
            signature: signer
                .sign_for_contract(ADDRESS, pub_key, incorrect_digest)
                .unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn check_origin_closure_behavior() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        // Case 1: check_origin allows the origin
        let result =
            multisigned_batch.accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()));
        assert!(result.is_ok());
        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Case 2: check_origin rejects the origin
        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| {
            anyhow::bail!("Origin not allowed")
        });
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 2);
    }

    #[test]
    fn test_aggregate_chain_commitment() {
        let db = Database::memory();
        let block1 = H256([1; 32]);
        let block2 = H256([2; 32]);
        let block3 = H256([3; 32]);

        // Set up the database with computed blocks and outcomes
        db.mutate_block_meta(block1, |meta| meta.computed = true);
        db.mutate_block_meta(block2, |meta| meta.computed = true);
        db.set_block_outcome(block1, vec![]);
        db.set_block_outcome(block2, vec![]);

        // Test with valid blocks
        aggregate_chain_commitment(&db, vec![block1, block2], true)
            .unwrap()
            .unwrap();

        // Test with a block that is not computed
        aggregate_chain_commitment(&db, vec![block1, block3], true).unwrap_err();

        // Test with fail_if_not_computed set to false
        let chain_commitment =
            aggregate_chain_commitment(&db, vec![block1, block3], false).unwrap();
        assert!(chain_commitment.is_none());
    }

    #[test]
    fn test_squash_chain_commitments() {
        let block1 = H256::from([1; 32]);
        let block2 = H256::from([2; 32]);

        let transition1 = StateTransition::mock(());
        let transition2 = StateTransition::mock(());
        let transition3 = StateTransition::mock(());

        let gb1 = GearBlock {
            hash: block1,
            off_chain_transactions_hash: H256::zero(),
            gas_allowance: 0,
        };
        let gb2 = GearBlock {
            hash: block2,
            off_chain_transactions_hash: H256::zero(),
            gas_allowance: 0,
        };
        let chain_commitment1 = ChainCommitment {
            transitions: vec![transition1.clone(), transition2.clone()],
            head_announce: vec![gb1.clone()],
        };

        let chain_commitment2 = ChainCommitment {
            transitions: vec![transition3.clone()],
            head_announce: vec![gb2.clone()],
        };

        let squashed =
            squash_chain_commitments(vec![chain_commitment1.clone(), chain_commitment2.clone()])
                .unwrap();

        assert_eq!(squashed.head_announce, vec![gb1, gb2]);
        assert_eq!(
            squashed.transitions,
            vec![transition1, transition2, transition3]
        );

        let squashed = squash_chain_commitments(vec![]);
        assert!(squashed.is_none());
    }

    #[test]
    fn test_aggregate_code_commitments() {
        let db = Database::memory();
        let codes = vec![CodeId::from([1; 32]), CodeId::from([2; 32])];

        // Test with valid codes
        db.set_code_valid(codes[0], true);
        db.set_code_valid(codes[1], false);

        let commitments = aggregate_code_commitments(&db, codes.clone(), false).unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        let commitments =
            aggregate_code_commitments(&db, vec![codes[0], CodeId::from([3; 32]), codes[1]], false)
                .unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        aggregate_code_commitments(&db, vec![CodeId::from([3; 32])], true).unwrap_err();
    }

    #[test]
    fn test_has_duplicates() {
        let data = vec![1, 2, 3, 4, 5];
        assert!(!has_duplicates(&data));

        let data = vec![1, 2, 3, 4, 5, 3];
        assert!(has_duplicates(&data));
    }
}
