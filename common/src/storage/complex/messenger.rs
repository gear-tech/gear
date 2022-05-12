use crate::storage::complex::Queue;
use crate::storage::complex::{Mailbox, MailboxError};
use crate::storage::complicated::{Counter, LinkedListError, Toggler};
use crate::storage::primitives::Counted;

/// Message processing centralized behaviour.
pub trait Messenger {
    type Capacity;
    type MailboxedMessage;
    type QueuedDispatch;
    type Error: MailboxError + LinkedListError;

    /// Amount of messages sent from outside.
    type Sent: Counter<Value = Self::Capacity>;

    /// Amount of messages dequeued.
    type Dequeued: Counter<Value = Self::Capacity>;

    /// Allowance of queue processing.
    type QueueProcessing: Toggler;

    /// Message queue store.
    type Queue: Queue<Value = Self::QueuedDispatch, Error = Self::Error>
        + Counted<Length = Self::Capacity>;

    // /// Users mailbox store.
    // type Mailbox: Mailbox<Value = Self::MailboxedMessage, Error = Self::Error>;
}
