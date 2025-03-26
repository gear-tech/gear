use anyhow::Result;
use ethexe_common::gear::{BatchCommitment, BlockCommitment, CodeCommitment};
use ethexe_signer::{
    sha3::digest::Update, Address, ContractSignature, ContractSigner, Digest, PublicKey, ToDigest,
};
use gprimitives::H256;
use std::collections::BTreeMap;

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
        self.blocks
            .iter()
            .for_each(|block| block.update_hasher(hasher));
        self.codes
            .iter()
            .for_each(|code| code.update_hasher(hasher));
    }
}

#[derive(Debug, Clone)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub block_timestamp: u64,
    pub previous_committed_block: H256,
    pub predecessor_block: H256,
    pub transitions_digest: Digest,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        BlockCommitmentValidationRequest {
            block_hash: commitment.hash,
            block_timestamp: commitment.timestamp,
            previous_committed_block: commitment.previous_committed_block,
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
        hasher.update(self.previous_committed_block.as_bytes());
        hasher.update(self.predecessor_block.as_bytes());
        hasher.update(self.transitions_digest.as_ref());
    }
}

pub struct BatchCommitmentValidationReply {
    pub digest: Digest,
    pub signature: ContractSignature,
}

pub struct MultisignedBatchCommitment {
    batch: BatchCommitment,
    batch_digest: Digest,
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

        let origin = signature.recover(digest)?.to_address();

        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    pub fn signatures(&self) -> &BTreeMap<Address, ContractSignature> {
        &self.signatures
    }

    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, ContractSignature>) {
        (self.batch, self.signatures)
    }
}

// TODO +_+_+: make test that signature for CommitmentsBatchValidationRequest is suitable for CommitmentsBatch as well
