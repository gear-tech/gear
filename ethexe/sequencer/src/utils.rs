use std::collections::BTreeMap;
use anyhow::Result;
use ethexe_common::gear::{BatchCommitment, BlockCommitment, CodeCommitment};
use ethexe_signer::{
    sha3::digest::Update, Address, Digest, PublicKey, Signature, SignedData, Signer, ToDigest,
};
use gprimitives::H256;

// pub type SignedCommitmentsBatch = SignedData<BatchCommitment>;

// +_+_+ we should use RouterSigner instead of Signer

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
    pub signature: Signature,
}

// impl BatchCommitmentValidationReply {
//     pub fn new(digest: Digest, signature: Signature) -> Self {
//         Self { digest, signature }
//     }
// }

pub struct MultisignedBatchCommitment {
    batch: BatchCommitment,
    batch_digest: Digest,
    signatures: BTreeMap<Address, Signature>,
}

impl MultisignedBatchCommitment {
    pub fn new_with_validation_request(
        batch: BatchCommitment,
        signer: &Signer,
        pub_key: PublicKey,
    ) -> Result<(Self, SignedData<BatchCommitmentValidationRequest>)> {
        let request = BatchCommitmentValidationRequest::from(&batch);
        let batch_digest = request.to_digest();
        let signed_request = signer.create_signed_data(pub_key, request)?;
        let signatures: BTreeMap<_, _> =
            [(pub_key.to_address(), signed_request.signature().clone())]
                .into_iter()
                .collect();

        Ok((
            Self {
                batch,
                batch_digest,
                signatures,
            },
            signed_request,
        ))
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

        let origin = reply.signature.recover_from_digest(digest)?.to_address();

        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    pub fn signatures(&self) -> &BTreeMap<Address, Signature> {
        &self.signatures
    }

    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, Signature>) {
        (self.batch, self.signatures)
    }
}

// TODO +_+_+: make test that signature for CommitmentsBatchValidationRequest is suitable for CommitmentsBatch as well
