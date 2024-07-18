use ethexe_common::events::BlockEvent;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub enum Event {
    CodeLoaded(CodeLoadedData),
    Block(BlockData),
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockData {
    pub parent_hash: H256,
    pub block_hash: H256,
    pub block_number: u64,
    pub block_timestamp: u64,
    pub events: Vec<BlockEvent>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CodeLoadedData {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub code: Vec<u8>,
}
