use gear_core::ids::{ActorId, CodeId, MessageId};
use gprimitives::H256;

#[derive(Debug)]
pub enum BlockEvent {
    CreateProgram {
        origin: ActorId,
        code_id: CodeId,
        salt: Vec<u8>,
        init_payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    },
    SendMessage {
        origin: ActorId,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    },
    SendReply {
        origin: ActorId,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    },
    ClaimValue {
        origin: ActorId,
        message_id: MessageId,
    },
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
