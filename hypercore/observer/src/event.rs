use crate::{Program, Router};

#[derive(Debug)]
pub enum Event {
    UploadCode(Router::UploadCode),
    CreateProgram(Router::CreateProgram),
    SendMessage(Program::SendMessage),
    SendReply(Program::SendReply),
    ClaimValue(Program::ClaimValue),
}
