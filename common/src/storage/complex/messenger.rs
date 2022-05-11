use crate::storage::complex::Queue;
use crate::storage::complex::{Mailbox, MailboxError};
use crate::storage::complicated::{Counter, LinkedListError, Toggler};
use crate::storage::primitives::Counted;

/// Message processing centralized behaviour.
pub trait Messenger {
    type QueueLength;
    type MailboxedMessage;
    type QueuedDispatch;
    type Error: MailboxError + LinkedListError;

    /// Amount of messages sent from outside.
    type Sent: Counter<Value = Self::QueueLength>;

    /// Amount of messages dequeued.
    type Dequeued: Counter<Value = Self::QueueLength>;

    /// Allowance of queue processing.
    type QueueProcessing: Toggler;

    /// Message queue store.
    type Queue: Queue<Value = Self::QueuedDispatch, Error = Self::Error>
        + Counted<Length = Self::QueueLength>;

    /// Users mailbox store.
    type Mailbox: Mailbox<Value = Self::MailboxedMessage, Error = Self::Error>;
}
