use crate::{
    bp::{ControlError, ControlEvent},
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        BlockCommitmentValidationRequest,
    },
};
use anyhow::{anyhow, ensure};
use ethexe_common::{gear::CodeCommitment, SimpleBlockData};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database, OnChainStorage};
use ethexe_signer::{Address, ContractSigner, Digest, PublicKey, SignedData, Signer, ToDigest};
use gprimitives::H256;

pub struct Participant {
    pub_key: PublicKey,
    producer: Address,
    db: Database,
    signer: ContractSigner,
    #[allow(unused)]
    state: State,
}

enum State {
    #[allow(unused)]
    WaitingForValidationRequest(SimpleBlockData),
}

impl Participant {
    pub fn new(
        pub_key: PublicKey,
        router_address: Address,
        producer: Address,
        block: SimpleBlockData,
        db: Database,
        signer: Signer,
    ) -> Self {
        Self {
            pub_key,
            producer,
            db,
            signer: signer.contract_signer(router_address),
            state: State::WaitingForValidationRequest(block),
        }
    }

    pub fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<Vec<ControlEvent>, ControlError> {
        request.verify_address(self.producer).map_err(|e| {
            ControlError::Warning(anyhow!(
                "Received validation request is not signed by the producer: {e}"
            ))
        })?;

        self.receive_validation_request_unsigned(request.into_parts().0)
    }

    pub fn receive_validation_request_unsigned(
        &mut self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<Vec<ControlEvent>, ControlError> {
        self.receive_validation_request_inner(request)
            .map(|reply| vec![ControlEvent::PublishValidationReply(reply)])
    }

    fn receive_validation_request_inner(
        &self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<BatchCommitmentValidationReply, ControlError> {
        let digest = request.to_digest();
        let BatchCommitmentValidationRequest { blocks, codes } = request;

        for code_request in codes {
            log::debug!("Receive code commitment for validation: {code_request:?}");
            Self::validate_code_commitment(&self.db, code_request).map_err(|e| {
                ControlError::Warning(anyhow!("Received code commitment is not valid: {e}"))
            })?;
        }

        for block_request in blocks {
            log::debug!("Receive block commitment for validation: {block_request:?}");
            Self::validate_block_commitment(&self.db, block_request).map_err(|e| {
                ControlError::Warning(anyhow!("Received block commitment is not valid: {e}"))
            })?;
        }

        self.signer
            .sign_digest(self.pub_key, digest)
            .map(|signature| BatchCommitmentValidationReply { digest, signature })
            .map_err(Into::into)
    }

    fn validate_code_commitment<DB1: OnChainStorage + CodesStorage>(
        db: &DB1,
        request: CodeCommitment,
    ) -> anyhow::Result<()> {
        let CodeCommitment {
            id,
            timestamp,
            valid,
        } = request;

        let local_timestamp = db
            .code_blob_info(id)
            .ok_or_else(|| anyhow!("Code {id} blob info is not in storage"))?
            .timestamp;

        if local_timestamp != timestamp {
            return Err(anyhow!("Requested and local code timestamps mismatch"));
        }

        let local_valid = db
            .code_valid(id)
            .ok_or_else(|| anyhow!("Code {id} is not validated by this node"))?;

        if local_valid != valid {
            return Err(anyhow!(
                "Requested and local code validation results mismatch"
            ));
        }

        Ok(())
    }

    fn validate_block_commitment<DB1: BlockMetaStorage + OnChainStorage>(
        db: &DB1,
        request: BlockCommitmentValidationRequest,
    ) -> anyhow::Result<()> {
        let BlockCommitmentValidationRequest {
            block_hash,
            block_timestamp,
            previous_committed_block: allowed_previous_committed_block,
            predecessor_block: allowed_predecessor_block,
            transitions_digest,
        } = request;

        if !db.block_computed(block_hash) {
            return Err(anyhow!(
                "Requested block {block_hash} is not processed by this node"
            ));
        }

        let header = db.block_header(block_hash).ok_or_else(|| {
            anyhow!("Requested block {block_hash} header wasn't found in storage")
        })?;

        ensure!(header.timestamp == block_timestamp, "Timestamps mismatch");

        if db
            .block_outcome(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db outcome for block {block_hash}"))?
            .iter()
            .collect::<Digest>()
            != transitions_digest
        {
            return Err(anyhow!("Requested and local transitions digest mismatch"));
        }

        if db.previous_not_empty_block(block_hash).ok_or_else(|| {
            anyhow!("Cannot get from db previous not empty for block {block_hash}")
        })? != allowed_previous_committed_block
        {
            return Err(anyhow!(
                "Requested and local previous commitment block hash mismatch"
            ));
        }

        // TODO +_+_+: rename max_distance and make it configurable
        if !Self::verify_is_predecessor(db, allowed_predecessor_block, block_hash, None)? {
            return Err(anyhow!(
                "{block_hash} is not a predecessor of {allowed_predecessor_block}"
            ));
        }

        Ok(())
    }

    /// Verify whether `pred_hash` is a predecessor of `block_hash` in the chain.
    fn verify_is_predecessor(
        db: &impl OnChainStorage,
        block_hash: H256,
        pred_hash: H256,
        max_distance: Option<u32>,
    ) -> anyhow::Result<bool> {
        if block_hash == pred_hash {
            return Ok(true);
        }

        let block_header = db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?;

        if block_header.parent_hash == pred_hash {
            return Ok(true);
        }

        let pred_height = db
            .block_header(pred_hash)
            .ok_or_else(|| anyhow!("header not found for pred block: {pred_hash}"))?
            .height;

        let distance = block_header.height.saturating_sub(pred_height);
        if max_distance.map(|d| d < distance).unwrap_or(false) {
            return Err(anyhow!("distance is too large: {distance}"));
        }

        let mut block_hash = block_hash;
        for _ in 0..=distance {
            if block_hash == pred_hash {
                return Ok(true);
            }
            block_hash = db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }
}
