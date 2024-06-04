use gprimitives::{ActorId, CodeId, H256};
pub use hypercore_ethereum::event::{ClaimValue, CreateProgram, SendMessage, SendReply};

#[derive(Debug)]
pub enum BlockEvent {
    CreateProgram(CreateProgram),
    SendMessage(SendMessage),
    SendReply(SendReply),
    ClaimValue(ClaimValue),
}

#[derive(Debug)]
pub enum Event {
    UploadCode {
        origin: ActorId,
        code_id: CodeId,
        code: Vec<u8>,
    },
    Block {
        block_hash: H256,
        events: Vec<BlockEvent>,
    },
}
