use gprimitives::{ActorId, CodeId, H256};
pub use hypercore_ethereum::event::{
    ClaimValue, CodeApproved, CodeRejected, CreateProgram, SendMessage, SendReply, UpdatedProgram,
    UploadCode, UserMessageSent, UserReplySent,
};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Encode, Decode)]
pub enum BlockEvent {
    CodeApproved(CodeApproved),
    CodeRejected(CodeRejected),
    CreateProgram(CreateProgram),
    UserMessageSent(UserMessageSent),
    UserReplySent(UserReplySent),
    UpdatedProgram(UpdatedProgram),
    SendMessage(SendMessage),
    SendReply(SendReply),
    ClaimValue(ClaimValue),
}

#[derive(Debug, Encode, Decode)]
pub struct BlockEventData {
    pub parent_hash: H256,
    pub block_hash: H256,
    pub block_number: u64,
    pub block_timestamp: u64,
    pub events: Vec<BlockEvent>,
}

#[derive(Debug, Encode, Decode)]
pub enum Event {
    UploadCode {
        origin: ActorId,
        code_id: CodeId,
        code: Vec<u8>,
    },
    Block(BlockEventData),
}
