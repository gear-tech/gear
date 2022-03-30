use std::cell::RefCell;
use codec::Encode;
use gear_core::{
    message::{Message, MessageId, Payload},
    program::ProgramId
};
use crate::{CoreLog, Log, RunResult, manager::ExtManager};

pub struct Mailbox<'a> {
    manager_reference: &'a RefCell<ExtManager>,
    program_id: ProgramId,
}

impl<'a> Mailbox<'a> {
    pub(crate) fn new(
        program_id: ProgramId,
        manager_reference: &'a RefCell<ExtManager>,
    ) -> Mailbox<'a> {
        Mailbox {
            program_id,
            manager_reference,
        }
    }

    pub fn contains(&self, log: Log) -> bool {
        self.manager_reference
            .borrow_mut()
            .actor_to_mailbox
            .get(&self.program_id)
            .expect("No mailbox with such program id")
            .iter()
            .any(|message| log.eq(message))
    }

    pub fn take_message(&self, log: Log) -> MessageReplier {
        let index = self
            .manager_reference
            .borrow_mut()
            .actor_to_mailbox
            .get(&self.program_id)
            .expect("No mailbox with such program id")
            .iter()
            .position(|message| log.eq(message))
            .expect("No message that satisfies log");

        let taken_message = self
            .manager_reference
            .borrow_mut()
            .take_message(&self.program_id, index);

        MessageReplier::new(taken_message, self.manager_reference)
    }

    pub fn reply(&self, log: Log, payload: impl Encode, value: u128) -> RunResult {
        self.take_message(log).reply(payload, value)
    }

    pub fn reply_bytes(&self, log: Log, raw_payload: impl AsRef<[u8]>, value: u128) -> RunResult {
        self.take_message(log).reply_bytes(raw_payload, value)
    }
}

pub struct MessageReplier<'a> {
    log: CoreLog,
    manager_reference: &'a RefCell<ExtManager>,
}

impl<'a> MessageReplier<'a> {
    pub(crate) fn new(
        message: Message,
        manager_reference: &'a RefCell<ExtManager>,
    ) -> MessageReplier<'a> {
        MessageReplier {
            log: CoreLog::from_message(message),
            manager_reference,
        }
    }

    pub fn reply(&self, payload: impl Encode, value: u128) -> RunResult {
        let message = self.log.generate_reply(
            payload.encode().into(),
            MessageId::from(
                self.manager_reference
                    .borrow_mut()
                    .fetch_inc_message_nonce(),
            ),
            value,
        );
        self.manager_reference.borrow_mut().run_message(message)
    }

    pub fn reply_bytes(&self, raw_payload: impl AsRef<[u8]>, value: u128) -> RunResult {
        let payload: Payload = raw_payload.as_ref().to_vec().into();
        self.reply(payload, value)
    }
}
