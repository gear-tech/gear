use gprimitives::{ActorId, CodeId, H256};
pub use hypercore_ethereum::event::{
    ClaimValue, CreateProgram, SendMessage, SendReply, UpdatedProgram,
};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Encode, Decode)]
pub enum BlockEvent {
    CreateProgram(CreateProgram),
    UpdatedProgram(UpdatedProgram),
    SendMessage(SendMessage),
    SendReply(SendReply),
    ClaimValue(ClaimValue),
}

#[derive(Debug, Encode, Decode)]
pub enum Event {
    UploadCode {
        origin: ActorId,
        code_id: CodeId,
        code: Vec<u8>,
    },
    Block {
        block_hash: H256,
        parent_hash: H256,
        block_number: u64,
        timestamp: u64,
        events: Vec<BlockEvent>,
    },
}
