use super::*;

/// Message processing centralized behaviour.
pub trait Messenger {
    /// Amount of messages sent from outside.
    type Sent: StorageCounter;

    /// Amount of messages dequeued.
    type Dequeued: StorageCounter;

    /// Allowance of queue processing.
    type QueueProcessing: StorageFlag;

    /// Message queue store.
    type Queue: StorageDeque;
}
