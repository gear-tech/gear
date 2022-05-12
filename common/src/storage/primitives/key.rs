use gear_core::{
    ids::{MessageId, ProgramId},
    message::{StoredDispatch, StoredMessage},
};

pub trait KeyFor {
    type Key;
    type Value;

    fn key_for(value: &Self::Value) -> Self::Key;
}

pub struct QueueKeyGen;

impl KeyFor for QueueKeyGen {
    type Key = MessageId;
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        value.id()
    }
}

pub struct MailboxKeyGen;

impl KeyFor for MailboxKeyGen {
    type Key = (ProgramId, MessageId);
    type Value = StoredMessage;

    fn key_for(value: &Self::Value) -> Self::Key {
        (value.destination(), value.id())
    }
}
