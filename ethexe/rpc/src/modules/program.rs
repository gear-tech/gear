use super::block::BlockApiModule;
use crate::errors;
use ethexe_db::{CodesStorage, Database};
use ethexe_processor::Processor;
use gear_core::message::ReplyInfo;
use gprimitives::{H160, H256};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use sp_core::Bytes;

#[rpc(server)]
pub trait Program {
    #[method(name = "program_calculateReplyForHandle")]
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo>;

    #[method(name = "program_ids")]
    async fn ids(&self) -> RpcResult<Vec<H160>>;
}

pub struct ProgramApiModule {
    db: Database,
    block: BlockApiModule,
}

impl ProgramApiModule {
    pub fn new(db: Database, block: BlockApiModule) -> Self {
        Self { db, block }
    }
}

#[async_trait]
impl ProgramServer for ProgramApiModule {
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo> {
        let block_hash = self.block.block_header_at_or_latest(at)?.0;

        // TODO (breathx): spawn in a new thread and catch panics. (?) Generally catch runtime panics (?).
        // TODO (breathx): optimize here instantiation if matches actual runtime.
        let processor = Processor::new(self.db.clone()).map_err(|_| errors::internal())?;

        let mut overlaid_processor = processor.overlaid();

        overlaid_processor
            .execute_for_reply(
                block_hash,
                source.into(),
                program_id.into(),
                payload.0,
                value,
            )
            .map_err(errors::runtime_err)
    }

    async fn ids(&self) -> RpcResult<Vec<H160>> {
        Ok(self
            .db
            .program_ids()
            .into_iter()
            .map(|id| id.into())
            .collect())
    }
}
