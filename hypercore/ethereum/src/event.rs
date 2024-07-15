use alloy::{primitives::LogData, rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use gear_core::message::ReplyDetails;
use gear_core_errors::ReplyCode;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use hypercore_common::events::*;

use crate::IRouter;

pub mod signature_hash {
    use super::{IRouter, SolEvent};

    pub const UPLOAD_CODE: [u8; 32] = IRouter::UploadCode::SIGNATURE_HASH.0;
    pub const CODE_APPROVED: [u8; 32] = IRouter::CodeApproved::SIGNATURE_HASH.0;
    pub const CODE_REJECTED: [u8; 32] = IRouter::CodeRejected::SIGNATURE_HASH.0;
    pub const CREATE_PROGRAM: [u8; 32] = IRouter::CreateProgram::SIGNATURE_HASH.0;
    pub const UPDATED_PROGRAM: [u8; 32] = IRouter::UpdatedProgram::SIGNATURE_HASH.0;
    pub const USER_MESSAGE_SENT: [u8; 32] = IRouter::UserMessageSent::SIGNATURE_HASH.0;
    pub const USER_REPLY_SENT: [u8; 32] = IRouter::UserReplySent::SIGNATURE_HASH.0;
    pub const SEND_MESSAGE: [u8; 32] = IRouter::SendMessage::SIGNATURE_HASH.0;
    pub const SEND_REPLY: [u8; 32] = IRouter::SendReply::SIGNATURE_HASH.0;
    pub const CLAIM_VALUE: [u8; 32] = IRouter::ClaimValue::SIGNATURE_HASH.0;
    pub const BLOCK_COMMITTED: [u8; 32] = IRouter::BlockCommitted::SIGNATURE_HASH.0;

    pub const ROUTER_EVENTS: [[u8; 32]; 11] = [
        UPLOAD_CODE,
        CODE_APPROVED,
        CODE_REJECTED,
        CREATE_PROGRAM,
        UPDATED_PROGRAM,
        USER_MESSAGE_SENT,
        USER_REPLY_SENT,
        SEND_MESSAGE,
        SEND_REPLY,
        CLAIM_VALUE,
        BLOCK_COMMITTED,
    ];
}

pub fn match_log(log: &Log) -> Result<Option<BlockEvent>> {
    use signature_hash::*;

    let event: BlockEvent = match log.topic0().copied().map(|bytes| bytes.0) {
        Some(UPLOAD_CODE) => {
            let event = UploadCode::from(decode_log::<IRouter::UploadCode>(log)?);
            PendingUploadCode {
                origin: event.origin,
                code_id: event.code_id,
                blob_tx: event.blob_tx,
                tx_hash: H256(
                    log.transaction_hash
                        .ok_or(anyhow!("Transaction hash not found"))?
                        .0,
                ),
            }
            .into()
        }
        Some(CODE_APPROVED) => CodeApproved::from(decode_log::<IRouter::CodeApproved>(log)?).into(),
        Some(CODE_REJECTED) => CodeRejected::from(decode_log::<IRouter::CodeRejected>(log)?).into(),
        Some(CREATE_PROGRAM) => {
            CreateProgram::from(decode_log::<IRouter::CreateProgram>(log)?).into()
        }
        Some(UPDATED_PROGRAM) => {
            UpdatedProgram::from(decode_log::<IRouter::UpdatedProgram>(log)?).into()
        }
        Some(USER_MESSAGE_SENT) => {
            UserMessageSent::from(decode_log::<IRouter::UserMessageSent>(log)?).into()
        }
        Some(USER_REPLY_SENT) => {
            UserReplySent::from(decode_log::<IRouter::UserReplySent>(log)?).into()
        }
        Some(SEND_MESSAGE) => SendMessage::from(decode_log::<IRouter::SendMessage>(log)?).into(),
        Some(SEND_REPLY) => SendReply::from(decode_log::<IRouter::SendReply>(log)?).into(),
        Some(CLAIM_VALUE) => ClaimValue::from(decode_log::<IRouter::ClaimValue>(log)?).into(),
        Some(BLOCK_COMMITTED) => {
            BlockCommitted::from(decode_log::<IRouter::BlockCommitted>(log)?).into()
        }
        Some(hash) => {
            log::warn!("Unknown event signature hash: {:?}", hash);
            return Ok(None);
        }
        None => {
            log::warn!("Event log has no topic0");
            return Ok(None);
        }
    };

    Ok(Some(event))
}

pub fn decode_log<E: SolEvent>(log: &Log) -> Result<E> {
    let log_data: &LogData = log.as_ref();
    E::decode_raw_log(log_data.topics().iter().copied(), &log_data.data, false).map_err(Into::into)
}

impl From<IRouter::UploadCode> for UploadCode {
    fn from(event: IRouter::UploadCode) -> Self {
        Self {
            origin: ActorId::new(event.origin.into_word().0),
            code_id: CodeId::new(event.codeId.0),
            blob_tx: H256(event.blobTx.0),
        }
    }
}

impl From<IRouter::CodeApproved> for CodeApproved {
    fn from(event: IRouter::CodeApproved) -> Self {
        Self {
            code_id: CodeId::new(event.codeId.0),
        }
    }
}

impl From<IRouter::CodeRejected> for CodeRejected {
    fn from(event: IRouter::CodeRejected) -> Self {
        Self {
            code_id: CodeId::new(event.codeId.0),
        }
    }
}

impl From<IRouter::CreateProgram> for CreateProgram {
    fn from(event: IRouter::CreateProgram) -> Self {
        Self {
            origin: ActorId::new(event.origin.into_word().0),
            actor_id: ActorId::new(event.actorId.into_word().0),
            code_id: CodeId::new(event.codeId.0),
            init_payload: event.initPayload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        }
    }
}

impl From<IRouter::UpdatedProgram> for UpdatedProgram {
    fn from(event: IRouter::UpdatedProgram) -> Self {
        Self {
            actor_id: ActorId::new(event.actorId.into_word().0),
            old_state_hash: H256(event.oldStateHash.0),
            new_state_hash: H256(event.newStateHash.0),
        }
    }
}

impl From<IRouter::UserMessageSent> for UserMessageSent {
    fn from(event: IRouter::UserMessageSent) -> Self {
        Self {
            destination: ActorId::new(event.destination.into_word().0),
            payload: event.payload.to_vec(),
            value: event.value,
        }
    }
}

impl From<IRouter::UserReplySent> for UserReplySent {
    fn from(event: IRouter::UserReplySent) -> Self {
        Self {
            destination: ActorId::new(event.destination.into_word().0),
            payload: event.payload.to_vec(),
            value: event.value,
            reply_details: ReplyDetails::new(
                MessageId::new(event.replyTo.0),
                ReplyCode::from_bytes(event.replyCode.0),
            ),
        }
    }
}

impl From<IRouter::SendMessage> for SendMessage {
    fn from(event: IRouter::SendMessage) -> Self {
        Self {
            origin: ActorId::new(event.origin.into_word().0),
            destination: ActorId::new(event.destination.into_word().0),
            payload: event.payload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        }
    }
}

impl From<IRouter::SendReply> for SendReply {
    fn from(event: IRouter::SendReply) -> Self {
        Self {
            origin: ActorId::new(event.origin.into_word().0),
            reply_to_id: MessageId::new(event.replyToId.0),
            payload: event.payload.to_vec(),
            gas_limit: event.gasLimit,
            value: event.value,
        }
    }
}

impl From<IRouter::ClaimValue> for ClaimValue {
    fn from(event: IRouter::ClaimValue) -> Self {
        Self {
            origin: ActorId::new(event.origin.into_word().0),
            message_id: MessageId::new(event.messageId.0),
        }
    }
}

impl From<IRouter::BlockCommitted> for BlockCommitted {
    fn from(event: IRouter::BlockCommitted) -> Self {
        Self {
            block_hash: H256(event.blockHash.0),
        }
    }
}
