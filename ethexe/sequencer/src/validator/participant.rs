use anyhow::{anyhow, ensure, Result};
use ethexe_common::{gear::CodeCommitment, SimpleBlockData};
use ethexe_db::{BlockMetaStorage, CodesStorage, OnChainStorage};
use ethexe_signer::{Address, Digest, ToDigest};
use gprimitives::H256;

use super::{initial::Initial, InputEvent, ValidatorContext, ValidatorSubService};
use crate::{
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        BlockCommitmentValidationRequest,
    },
    ControlEvent,
};

pub struct Participant {
    ctx: ValidatorContext,
    #[allow(unused)]
    block: SimpleBlockData,
    producer: Address,
}

impl ValidatorSubService for Participant {
    fn log(&self, s: String) -> String {
        format!("PARTICIPANT - {s}")
    }

    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_input_event(
        mut self: Box<Self>,
        event: InputEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match event {
            InputEvent::ValidationRequest(request)
                if request.verify_address(self.producer).is_ok() =>
            {
                self.process_validation_request(request.into_parts().0)
            }
            event => {
                let warning = format!("unexpected event: {event:?}, saved for later");
                self.ctx.warning(warning);

                self.ctx.pending_events.push_back(event);

                Ok(self)
            }
        }
    }
}

impl Participant {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let mut earlier_validation_request = None;

        ctx.pending_events.retain(|event| match event {
            InputEvent::ValidationRequest(signed_data)
                if earlier_validation_request.is_none()
                    && signed_data.verify_address(producer).is_ok() =>
            {
                earlier_validation_request = Some(signed_data.data().clone());

                false
            }
            InputEvent::ValidationRequest(_) if earlier_validation_request.is_none() => {
                // NOTE: remove all validation events before the first from producer found.
                false
            }
            _ => {
                // NOTE: keep all other events in queue.
                // Newer validation events could be from next block producer, so better to keep them.
                true
            }
        });

        let participant = Box::new(Self {
            ctx,
            block,
            producer,
        });

        let Some(validation_request) = earlier_validation_request else {
            return Ok(participant);
        };

        participant.process_validation_request(validation_request)
    }

    fn process_validation_request(
        mut self: Box<Self>,
        request: BatchCommitmentValidationRequest,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match self.process_validation_request_inner(request) {
            Ok(reply) => {
                self.ctx
                    .output
                    .push_back(ControlEvent::PublishValidationReply(reply));

                Initial::create(self.ctx)
            }
            Err(err) => {
                let warning = self.log(format!("reject validation request: {err}"));
                self.ctx.warning(warning);

                Ok(self)
            }
        }
    }

    fn process_validation_request_inner(
        &self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<BatchCommitmentValidationReply> {
        let digest = request.to_digest();
        let BatchCommitmentValidationRequest { blocks, codes } = request;

        for code_request in codes {
            log::debug!("Receive code commitment for validation: {code_request:?}");
            Self::validate_code_commitment(&self.ctx.db, code_request)?;
        }

        for block_request in blocks {
            log::debug!("Receive block commitment for validation: {block_request:?}");
            Self::validate_block_commitment(&self.ctx.db, block_request)?;
        }

        self.ctx
            .signer
            .contract_signer(self.ctx.router_address)
            .sign_digest(self.ctx.pub_key, digest)
            .map(|signature| BatchCommitmentValidationReply { digest, signature })
    }

    fn validate_code_commitment<DB1: OnChainStorage + CodesStorage>(
        db: &DB1,
        request: CodeCommitment,
    ) -> Result<()> {
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
    ) -> Result<()> {
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

        // TODO: #4579 rename max_distance and make it configurable
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
    ) -> Result<bool> {
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
