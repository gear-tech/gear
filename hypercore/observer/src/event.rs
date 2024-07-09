use gprimitives::{ActorId, CodeId, H256};
pub use hypercore_ethereum::event::{
    BlockCommitted, ClaimValue, CodeApproved, CodeRejected, CreateProgram, SendMessage, SendReply,
    UpdatedProgram, UploadCode, UserMessageSent, UserReplySent,
};
use parity_scale_codec::{Decode, Encode};

use crate::observer::PendingUploadCode;

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
    BlockCommitted(BlockCommitted),
}

#[derive(Debug, Encode, Decode)]
pub struct BlockEventData {
    pub parent_hash: H256,
    pub block_hash: H256,
    pub block_number: u64,
    pub block_timestamp: u64,
    pub events: Vec<BlockEvent>,
    pub upload_codes: Vec<PendingUploadCode>,
}

#[derive(Debug, Encode, Decode)]
pub struct UploadCodeData {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub code: Vec<u8>,
}

#[derive(Debug, Encode, Decode)]
pub enum Event {
    UploadCode(UploadCodeData),
    Block(BlockEventData),
}
