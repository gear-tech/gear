// TODO +_+_+: doc

mod connect;
mod utils;
mod validator;

#[cfg(test)]
mod tests;

pub use connect::SimpleConnectService;
pub use utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
pub use validator::{ValidatorConfig, ValidatorService};

use anyhow::Result;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_observer::BlockSyncedData;
use ethexe_signer::SignedData;
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;

pub trait ControlService:
    Stream<Item = Result<ControlEvent>> + FusedStream + Unpin + Send + 'static
{
    fn role(&self) -> String;
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()>;
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()>;
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<()>;
    fn receive_block_from_producer(&mut self, block: SignedData<ProducerBlock>) -> Result<()>;
    fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()>;
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlEvent {
    ComputeBlock(H256),
    ComputeProducerBlock(ProducerBlock),
    PublishProducerBlock(SignedData<ProducerBlock>),
    PublishValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    PublishValidationReply(BatchCommitmentValidationReply),
    CommitmentSubmitted(H256),
    Warning(String),
}
