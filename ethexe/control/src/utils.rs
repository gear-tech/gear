use anyhow::Result;
use ethexe_common::gear::{BatchCommitment, BlockCommitment, CodeCommitment};
use ethexe_signer::{
    sha3::digest::Update, Address, ContractSignature, ContractSigner, Digest, PublicKey, ToDigest,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationRequest {
    pub blocks: Vec<BlockCommitmentValidationRequest>,
    pub codes: Vec<CodeCommitment>,
}

impl From<&BatchCommitment> for BatchCommitmentValidationRequest {
    fn from(batch: &BatchCommitment) -> Self {
        BatchCommitmentValidationRequest {
            blocks: batch
                .block_commitments
                .iter()
                .map(BlockCommitmentValidationRequest::from)
                .collect(),
            codes: batch.code_commitments.clone(),
        }
    }
}

impl ToDigest for BatchCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut ethexe_signer::sha3::Keccak256) {
        hasher.update(self.codes.to_digest().as_ref());
        hasher.update(self.blocks.to_digest().as_ref());
    }
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub block_timestamp: u64,
    pub previous_not_empty_block: H256,
    pub predecessor_block: H256,
    pub transitions_digest: Digest,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        BlockCommitmentValidationRequest {
            block_hash: commitment.hash,
            block_timestamp: commitment.timestamp,
            previous_not_empty_block: commitment.previous_committed_block,
            predecessor_block: commitment.predecessor_block,
            transitions_digest: commitment.transitions.to_digest(),
        }
    }
}

impl ToDigest for BlockCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut ethexe_signer::sha3::Keccak256) {
        hasher.update(self.block_hash.as_bytes());
        hasher
            .update(ethexe_common::u64_into_uint48_be_bytes_lossy(self.block_timestamp).as_slice());
        hasher.update(self.previous_not_empty_block.as_bytes());
        hasher.update(self.predecessor_block.as_bytes());
        hasher.update(self.transitions_digest.as_ref());
    }
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationReply {
    pub digest: Digest,
    pub signature: ContractSignature,
}

pub struct MultisignedBatchCommitment {
    batch: BatchCommitment,
    batch_digest: Digest,
    router_address: Address,
    signatures: BTreeMap<Address, ContractSignature>,
}

impl MultisignedBatchCommitment {
    pub fn new(
        batch: BatchCommitment,
        signer: &ContractSigner,
        pub_key: PublicKey,
    ) -> Result<Self> {
        let batch_digest = batch.to_digest();
        let signature = signer.sign_digest(pub_key, batch_digest)?;
        let signatures: BTreeMap<_, _> = [(pub_key.to_address(), signature)].into_iter().collect();

        Ok(Self {
            batch,
            batch_digest,
            router_address: signer.contract_address(),
            signatures,
        })
    }

    pub fn accept_batch_commitment_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
        check_origin: impl FnOnce(Address) -> Result<()>,
    ) -> Result<()> {
        let BatchCommitmentValidationReply { digest, signature } = reply;

        if digest != self.batch_digest {
            anyhow::bail!("Invalid digest");
        }

        let origin = signature.recover(self.router_address, digest)?.to_address();

        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    pub fn signatures(&self) -> &BTreeMap<Address, ContractSignature> {
        &self.signatures
    }

    pub fn batch(&self) -> &BatchCommitment {
        &self.batch
    }

    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, ContractSignature>) {
        (self.batch, self.signatures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let signer = signer.contract_signer(Address([42; 20]));
        let pub_key = public_keys[0];

        let multisigned_batch = MultisignedBatchCommitment::new(batch.clone(), &signer, pub_key)
            .expect("Failed to create multisigned batch commitment");

        assert_eq!(multisigned_batch.batch, batch);
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn accept_batch_commitment_validation_reply() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let signer = signer.contract_signer(Address([42; 20]));
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_digest(other_pub_key, multisigned_batch.batch_digest)
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
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let signer = signer.contract_signer(Address([42; 20]));
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, pub_key).unwrap();

        let incorrect_digest = [1, 2, 3].as_slice().to_digest();
        let reply = BatchCommitmentValidationReply {
            digest: incorrect_digest,
            signature: signer.sign_digest(pub_key, incorrect_digest).unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn check_origin_closure_behavior() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let signer = signer.contract_signer(Address([42; 20]));
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_digest(other_pub_key, multisigned_batch.batch_digest)
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
    fn signature_compatibility_between_validation_request_and_batch_commitment() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![CodeCommitment {
                id: H256::random().into(),
                timestamp: 123,
                valid: false,
            }],
        };
        let batch_validation_request = BatchCommitmentValidationRequest::from(&batch);
        assert_eq!(batch.to_digest(), batch_validation_request.to_digest());

        let contract_address = Address([42; 20]);
        let (signer, _, public_keys) = init_signer_with_keys(1);
        let signer = signer.contract_signer(contract_address);
        let public_key = public_keys[0];

        let batch_signature = signer.sign_data(public_key, &batch).unwrap();
        let validation_request_signature = signer
            .sign_data(public_key, &batch_validation_request)
            .unwrap();
        assert_eq!(batch_signature, validation_request_signature);

        let pk1 = batch_signature
            .recover(contract_address, batch.to_digest())
            .unwrap();
        let pk2 = validation_request_signature
            .recover(contract_address, batch_validation_request.to_digest())
            .unwrap();
        assert_eq!(pk1, pk2);
    }
}
