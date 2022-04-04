use crate::{manager::ExtManager, CoreLog, Log, RunResult};
use codec::Encode;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, StoredMessage},
};
use std::cell::RefCell;

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

    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log: Log = log.clone().into();
        match self
            .manager_reference
            .borrow()
            .mailbox
            .get(&self.program_id)
        {
            None => {
                self.manager_reference
                    .borrow_mut()
                    .mailbox
                    .insert(self.program_id, Vec::default());
                false
            }
            Some(mailbox) => mailbox.iter().any(|message| log.eq(message)),
        }
    }

    pub fn take_message<T: Into<Log> + Clone>(&self, log: T) -> MessageReplier {
        let log: Log = log.into();
        let index = match self
            .manager_reference
            .borrow()
            .mailbox
            .get(&self.program_id)
        {
            None => {
                self.manager_reference
                    .borrow_mut()
                    .mailbox
                    .insert(self.program_id, Vec::default());
                panic!("No message that satisfies log");
            }
            Some(mailbox) => mailbox
                .iter()
                .position(|message| log.eq(message))
                .expect("No message that satisfies log"),
        };

        let taken_message = self
            .manager_reference
            .borrow_mut()
            .mailbox
            .get_mut(&self.program_id)
            .unwrap()
            .remove(index);

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
    manager: &'a RefCell<ExtManager>,
}

impl<'a> MessageReplier<'a> {
    pub(crate) fn new(
        message: StoredMessage,
        manager: &'a RefCell<ExtManager>,
    ) -> MessageReplier<'a> {
        MessageReplier {
            log: message.into(),
            manager,
        }
    }

    pub fn reply(&self, payload: impl Encode, value: u128) -> RunResult {
        self.reply_bytes(payload.encode(), value)
    }

    pub fn reply_bytes(&self, raw_payload: impl AsRef<[u8]>, value: u128) -> RunResult {
        let message = Message::new(
            MessageId::from(self.manager.borrow_mut().fetch_inc_message_nonce()),
            self.log.destination(),
            self.log.source(),
            raw_payload.as_ref().to_vec(),
            None,
            value,
            self.log
                .exit_code()
                .map(|exit_code| (self.log.id(), exit_code)),
        );

        self.manager
            .borrow_mut()
            .run_dispatch(Dispatch::new(DispatchKind::Reply, message))
    }
}

#[cfg(test)]
mod tests {
    use crate::program::ProgramIdWrapper;
    use crate::{Log, System};
    use codec::Encode;
    use gear_core::{
        ids::MessageId,
        message::{Dispatch, DispatchKind, Message, Payload},
    };

    #[test]
    fn mailbox_mock_walkthrough_test() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let encoded_message_payload: Payload = message_payload.encode().into();
        let reply_payload: Payload = vec![3, 2, 1].into();
        let log = Log::builder().payload(message_payload.clone());
        let encoded_reply_payload: Payload = reply_payload.encode().into();

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            encoded_message_payload.clone(),
            Default::default(),
            2,
            None,
        );

        let message_result =
            system.send_dispatch(Dispatch::new(DispatchKind::Handle, message.clone()));
        let message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        let message_replier = destination_user_mailbox.take_message(log);
        let reply_log = message_replier.reply(reply_payload.clone(), 1).log;

        let last_reply_log = reply_log.last().expect("No message log in run result");

        let second_message_result =
            system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        let second_message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        assert!(!message_result.main_failed);
        assert!(!message_result.others_failed);
        assert!(!second_message_result.main_failed);
        assert!(!second_message_result.others_failed);
        assert_eq!(reply_log.len(), 1);
        assert_eq!(last_reply_log.payload(), encoded_reply_payload);
        assert_eq!(message_log.payload(), encoded_message_payload);
        assert_eq!(second_message_log.payload(), encoded_message_payload);
    }

    #[test]
    fn mailbox_mock_deletes_message_after_reply() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let reply_payload: Payload = vec![3, 2, 1].into();
        let message_log = Log::builder().payload(message_payload.clone());

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().into(),
            Default::default(),
            2,
            None,
        );

        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        let mut destination_user_mailbox = system.get_mailbox(destination_user_id);
        let message_replier = destination_user_mailbox.take_message(message_log.clone());
        message_replier.reply(reply_payload, 1);

        destination_user_mailbox = system.get_mailbox(destination_user_id);
        assert!(!destination_user_mailbox.contains(&message_log))
    }

    #[test]
    fn mailbox_mock_reply_bytes_test() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let reply_payload_array: [u8; 3] = [3, 2, 1];
        let reply_payload: Payload = reply_payload_array.to_vec().into();
        let log = Log::builder().payload(message_payload.clone());

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().into(),
            Default::default(),
            2,
            None,
        );

        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        let message_replier = destination_user_mailbox.take_message(log);

        let result = message_replier.reply_bytes(&reply_payload_array, 1);
        let result_log = result.log;
        let last_result_log = result_log.last().expect("No message log in run result");
        assert_eq!(last_result_log.payload(), reply_payload);
    }

    #[test]
    fn mailbox_mock_deletes_message_after_taking() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let log = Log::builder().payload(message_payload.clone());

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().into(),
            Default::default(),
            2,
            None,
        );

        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        destination_user_mailbox.take_message(log.clone());

        assert!(!destination_user_mailbox.contains(&log))
    }
}
