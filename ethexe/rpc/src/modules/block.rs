use std::collections::VecDeque;

use ethexe_common::BlockRequestEvent;
use ethexe_db::{BlockHeader, BlockMetaStorage, Database};
use gprimitives::H256;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};

use crate::errors;

pub fn block_header_at_or_latest(
    db: &Database,
    at: impl Into<Option<H256>>,
) -> RpcResult<(H256, BlockHeader)> {
    if let Some(hash) = at.into() {
        db.block_header(hash)
            .map(|header| (hash, header))
            .ok_or_else(|| errors::db_err("Block header for requested hash wasn't found"))
    } else {
        db.latest_valid_block()
            .ok_or_else(|| errors::db_err("Latest block header wasn't found"))
    }
}

#[rpc(server)]
pub trait Block {
    #[method(name = "block_header")]
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)>;

    #[method(name = "block_commitmentQueue")]
    async fn block_commitment_queue(&self, hash: Option<H256>) -> RpcResult<VecDeque<H256>>;

    #[method(name = "block_events")]
    async fn block_events(&self, block_hash: Option<H256>) -> RpcResult<Vec<BlockRequestEvent>>;
}

#[derive(Clone)]
pub struct BlockApiModule {
    db: Database,
}

impl BlockApiModule {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl BlockServer for BlockApiModule {
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)> {
        block_header_at_or_latest(&self.db, hash)
    }

    async fn block_commitment_queue(&self, hash: Option<H256>) -> RpcResult<VecDeque<H256>> {
        let block_hash = block_header_at_or_latest(&self.db, hash)?.0;

        self.db
            .block_commitment_queue(block_hash)
            .ok_or_else(|| errors::db_err("Block commitment queue wasn't found"))
    }

    async fn block_events(&self, hash: Option<H256>) -> RpcResult<Vec<BlockRequestEvent>> {
        let block_hash = block_header_at_or_latest(&self.db, hash)?.0;

        self.db
            .block_events(block_hash)
            .ok_or_else(|| errors::db_err("Block events weren't found"))
    }
}
