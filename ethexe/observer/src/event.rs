use ethexe_common::BlockEvent;
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub enum Event {
    Block(BlockData),
    CodeLoaded { code_id: CodeId, code: Vec<u8> },
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockData {
    pub parent_hash: H256,
    pub block_hash: H256,
    pub block_number: u64,
    pub block_timestamp: u64,
    pub events: Vec<BlockEvent>,
}
