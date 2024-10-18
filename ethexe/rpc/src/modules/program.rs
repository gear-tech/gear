use super::block::block_header_at_or_latest;
use crate::errors;
use ethexe_db::{CodesStorage, Database};
use ethexe_processor::Processor;
use ethexe_runtime_common::state::{
    Mailbox, MemoryPages, MessageQueue, ProgramState, Storage, Waitlist,
};
use gear_core::{memory::PageBuf, message::ReplyInfo};
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

    #[method(name = "program_codeId")]
    async fn code_id(&self, program_id: H160) -> RpcResult<H256>;

    #[method(name = "program_readState")]
    async fn read_state(&self, hash: H256) -> RpcResult<ProgramState>;

    #[method(name = "program_readQueue")]
    async fn read_queue(&self, hash: H256) -> RpcResult<MessageQueue>;

    #[method(name = "program_readMailbox")]
    async fn read_mailbox(&self, hash: H256) -> RpcResult<Mailbox>;

    #[method(name = "program_readPages")]
    async fn read_pages(&self, hash: H256) -> RpcResult<MemoryPages>;

    #[method(name = "program_readWaitlist")]
    async fn read_waitlist(&self, hash: H256) -> RpcResult<Waitlist>;

    #[method(name = "program_readPageData")]
    async fn read_page_data(&self, hash: H256) -> RpcResult<PageBuf>;
}

pub struct ProgramApiModule {
    db: Database,
}

impl ProgramApiModule {
    pub fn new(db: Database) -> Self {
        Self { db }
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
        let block_hash = block_header_at_or_latest(&self.db, at)?.0;

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

    async fn code_id(&self, program_id: H160) -> RpcResult<H256> {
        self.db
            .program_code_id(program_id.into())
            .ok_or_else(|| errors::db_err("Failed to get code id"))
            .map(|code_id| code_id.into())
    }

    async fn read_state(&self, hash: H256) -> RpcResult<ProgramState> {
        self.db
            .read_state(hash)
            .ok_or_else(|| errors::db_err("Failed to read state by hash"))
    }

    async fn read_queue(&self, hash: H256) -> RpcResult<MessageQueue> {
        self.db
            .read_queue(hash)
            .ok_or_else(|| errors::db_err("Failed to read queue by hash"))
    }

    async fn read_mailbox(&self, hash: H256) -> RpcResult<Mailbox> {
        self.db
            .read_mailbox(hash)
            .ok_or_else(|| errors::db_err("Failed to read mailbox by hash"))
    }

    async fn read_pages(&self, hash: H256) -> RpcResult<MemoryPages> {
        self.db
            .read_pages(hash)
            .ok_or_else(|| errors::db_err("Failed to read pages by hash"))
    }

    async fn read_waitlist(&self, hash: H256) -> RpcResult<Waitlist> {
        self.db
            .read_waitlist(hash)
            .ok_or_else(|| errors::db_err("Failed to read waitlist by hash"))
    }

    async fn read_page_data(&self, hash: H256) -> RpcResult<PageBuf> {
        self.db
            .read_page_data(hash)
            .ok_or_else(|| errors::db_err("Failed to read page data by hash"))
    }
}
