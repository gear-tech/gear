use crate::AlloyRouter;
use alloy::sol_types::SolEvent;
use gprimitives::{ActorId, CodeId, MessageId, H256};

#[derive(Debug)]
pub struct UploadCode {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub blob_tx: H256,
}

impl UploadCode {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::UploadCode::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for UploadCode {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event = AlloyRouter::UploadCode::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            origin: ActorId::new(event.origin.into_word().0),
            code_id: CodeId::new(event.codeId.0),
            blob_tx: H256(event.blobTx.0),
        })
    }
}

#[derive(Debug)]
pub struct UploadedCode {
    pub code_id: CodeId,
}

impl UploadedCode {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::UploadedCode::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for UploadedCode {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event = AlloyRouter::UploadedCode::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            code_id: CodeId::new(event.codeId.0),
        })
    }
}

#[derive(Debug)]
pub struct CreateProgram {
    pub origin: ActorId,
    pub actor_id: ActorId,
    pub code_id: CodeId,
    pub salt: H256,
    pub init_payload: Vec<u8>,
    pub gas_limit: u64,
    pub value: u128,
}

impl CreateProgram {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::CreateProgram::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for CreateProgram {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event =
            AlloyRouter::CreateProgram::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            origin: ActorId::new(event.origin.into_word().0),
            actor_id: ActorId::new(event.actorId.into_word().0),
            code_id: CodeId::new(event.codeId.0),
            salt: H256(event.salt.0),
            init_payload: event.initPayload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        })
    }
}

#[derive(Debug)]
pub struct UpdatedProgram {
    pub actor_id: ActorId,
    pub old_state_hash: H256,
    pub new_state_hash: H256,
}

impl UpdatedProgram {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::UpdatedProgram::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for UpdatedProgram {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event =
            AlloyRouter::UpdatedProgram::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            actor_id: ActorId::new(event.actorId.into_word().0),
            old_state_hash: H256(event.oldStateHash.0),
            new_state_hash: H256(event.newStateHash.0),
        })
    }
}

#[derive(Debug)]
pub struct SendMessage {
    origin: ActorId,
    destination: ActorId,
    payload: Vec<u8>,
    gas_limit: u64,
    value: u128,
}

impl SendMessage {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::SendMessage::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for SendMessage {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event = AlloyRouter::SendMessage::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            origin: ActorId::new(event.origin.into_word().0),
            destination: ActorId::new(event.destination.into_word().0),
            payload: event.payload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        })
    }
}

#[derive(Debug)]
pub struct SendReply {
    origin: ActorId,
    reply_to_id: MessageId,
    payload: Vec<u8>,
    gas_limit: u64,
    value: u128,
}

impl SendReply {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::SendReply::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for SendReply {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event = AlloyRouter::SendReply::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            origin: ActorId::new(event.origin.into_word().0),
            reply_to_id: MessageId::new(event.replyToId.0),
            payload: event.payload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        })
    }
}

#[derive(Debug)]
pub struct ClaimValue {
    origin: ActorId,
    message_id: MessageId,
}

impl ClaimValue {
    pub const SIGNATURE_HASH: [u8; 32] = AlloyRouter::ClaimValue::SIGNATURE_HASH.0;
}

impl TryFrom<&[u8]> for ClaimValue {
    type Error = anyhow::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let event = AlloyRouter::ClaimValue::decode_raw_log([Self::SIGNATURE_HASH], data, false)?;

        Ok(Self {
            origin: ActorId::new(event.origin.into_word().0),
            message_id: MessageId::new(event.messageId.0),
        })
    }
}
