// TODO +_+_+: doc

use crate::utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_observer::BlockSyncedData;
use ethexe_signer::SignedData;
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;

pub trait ControlService: Stream<Item = ControlEvent> + FusedStream {
    fn receive_new_chain_head(&mut self, block: SimpleBlockData);
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<(), ControlError>;
    fn receive_block_from_producer(
        &mut self,
        block: SignedData<ProducerBlock>,
    ) -> Result<(), ControlError>;
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<(), ControlError>;
    fn receive_validation_request(
        &mut self,
        signed_batch: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<(), ControlError>;
    fn receive_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<(), ControlError>;
    fn is_block_producer(&self) -> anyhow::Result<bool>;
}

#[derive(Debug, derive_more::From)]
pub enum ControlError {
    #[from]
    Fatal(anyhow::Error),
    Warning(anyhow::Error),
    EventSkipped,
}

pub enum ControlEvent {
    ComputeBlock(H256),
    ComputeProducerBlock(ProducerBlock),
    PublishProducerBlock(SignedData<ProducerBlock>),
    PublishValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    PublishValidationReply(BatchCommitmentValidationReply),
    SubmissionResult(Result<H256, ControlError>),
}
