use crate::{Program, Router};

use alloy::primitives::B256;

#[derive(Debug)]
pub enum Event {
    UploadCode(Router::UploadCode),
    CreateProgram(Router::CreateProgram),
    SendMessage(Program::SendMessage),
    SendReply(Program::SendReply),
    ClaimValue(Program::ClaimValue),
}

#[derive(Debug)]

pub struct EventsBlock {
    pub block_hash: B256,
    pub events: Vec<Event>,
}
