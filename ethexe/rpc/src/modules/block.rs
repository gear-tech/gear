use ethexe_db::{BlockHeader, BlockMetaStorage, Database};
use gprimitives::H256;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};

use crate::errors::db_err;

#[rpc(server)]
pub trait Block {
    #[method(name = "block_header")]
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)>;
}

#[derive(Clone)]
pub struct BlockApiModule {
    db: Database,
}

impl BlockApiModule {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn block_header_at_or_latest(
        &self,
        at: impl Into<Option<H256>>,
    ) -> RpcResult<(H256, BlockHeader)> {
        if let Some(hash) = at.into() {
            self.db
                .block_header(hash)
                .map(|header| (hash, header))
                .ok_or_else(|| db_err("Block header for requested hash wasn't found"))
        } else {
            self.db
                .latest_valid_block()
                .ok_or_else(|| db_err("Latest block header wasn't found"))
        }
    }
}

#[async_trait]
impl BlockServer for BlockApiModule {
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)> {
        self.block_header_at_or_latest(hash)
    }
}
