use alloy::{consensus::Block, primitives::Address};
use ethexe_ethereum::{
    Ethereum,
    abi::IMirror::{
        ExecutableBalanceTopUpRequested, MessageQueueingRequested, OwnedBalanceTopUpRequested,
        ReplyQueueingRequested, StateChanged, ValueClaimingRequested,
    },
};
use tokio::sync::broadcast::Receiver;

use crate::batch::context::Context;

pub mod context;

pub struct BatchPool {
    api: Ethereum,
    pool_size: usize,
    batch_size: usize,
    task_context: Context,
    rx: Receiver<Event>,
}

/// Events emmitted by mirror contract. Used to build mailbox and other context state for
/// batch report.
pub struct Event {
    pub kind: EventKind,
    /// Address of the contract that emitted the event
    pub address: Address,
}

pub enum EventKind {
    StateChanged(StateChanged),
    MessageQueueingRequested(MessageQueueingRequested),
    ReplyQueueingRequested(ReplyQueueingRequested),
    ValueClaimingRequested(ValueClaimingRequested),
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequested),
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequested),
    Message(ethexe_ethereum::abi::IMirror::Message),
    MessageCallFailed(ethexe_ethereum::abi::IMirror::MessageCallFailed),
    Reply(ethexe_ethereum::abi::IMirror::Reply),
    ReplyCallFailed(ethexe_ethereum::abi::IMirror::ReplyCallFailed),

    ValueClaimed(ethexe_ethereum::abi::IMirror::ValueClaimed),
}
