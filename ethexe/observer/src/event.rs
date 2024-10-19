use ethexe_common::{BlockEvent, BlockRequestEvent};
use ethexe_db::BlockHeader;
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub enum RequestEvent {
    Block(RequestBlockData),
    CodeLoaded { code_id: CodeId, code: Vec<u8> },
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum Event {
    Block(BlockData),
    CodeLoaded { code_id: CodeId, code: Vec<u8> },
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct RequestBlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockRequestEvent>,
}

impl RequestBlockData {
    pub fn as_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn as_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}
