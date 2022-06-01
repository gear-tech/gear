use codec::{Decode, Encode};
use gear_core::ids::MessageId;
use primitive_types::H256;
use scale_info::TypeInfo;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum Entry {
    Init,
    Handle,
    Reply(MessageId),
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum DispatchStatus {
    Success,
    Failed,
    NotExecuted,
}

pub trait RuntimeReason: Sized {
    fn into_reason<S: SystemReason>(self) -> Reason<Self, S> {
        Reason::Runtime(self)
    }
}

impl RuntimeReason for () {}

pub trait SystemReason: Sized {
    fn into_reason<R: RuntimeReason>(self) -> Reason<R, Self> {
        Reason::System(self)
    }
}

impl SystemReason for () {}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum Reason<R: RuntimeReason, S: SystemReason> {
    Runtime(R),
    System(S),
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum MessageWaitedRuntimeReason {
    WaitCalled,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum MessageWaitedSystemReason {
    DidNotFinishInit,
}

pub type MessageWaitedReason = Reason<MessageWaitedRuntimeReason, MessageWaitedSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum MessageWakenRuntimeReason {
    WakeCalled,
    TimeoutBecome,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum MessageWakenSystemReason {
    FailedInit,
    OutOfRent,
}

pub type MessageWakenReason = Reason<MessageWakenRuntimeReason, MessageWakenSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum CodeChangeKind<BlockNumber> {
    Active { expiration: Option<BlockNumber> },

    Inactive,

    Reinstrumented,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum UserMessageReadRuntimeReason {
    Replied,
    Claimed,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum UserMessageReadSystemReason {
    OutOfRent,
}

pub type UserMessageReadReason = Reason<UserMessageReadRuntimeReason, UserMessageReadSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum ProgramChangeKind<BlockNumber> {
    Active {
        expiration: BlockNumber,
    },

    Inactive,

    Paused {
        code_hash: H256,
        memory_hash: H256,
        waitlist_hash: H256,
    },

    StateChanged,
}
