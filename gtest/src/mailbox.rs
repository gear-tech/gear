// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{manager::ExtManager, CoreLog, Log, RunResult};
use codec::Encode;
use core_processor::common::JournalHandler;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, ReplyDetails, StoredMessage},
};
use std::{cell::RefCell, convert::TryInto};

pub struct Mailbox<'a> {
    manager: &'a RefCell<ExtManager>,
    user_id: ProgramId,
}

impl<'a> Mailbox<'a> {
    pub(crate) fn new(user_id: ProgramId, manager: &'a RefCell<ExtManager>) -> Mailbox<'a> {
        Mailbox { user_id, manager }
    }

    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log: Log = log.clone().into();
        if let Some(mailbox) = self.manager.borrow().mailbox.get(&self.user_id) {
            return mailbox.iter().any(|message| log.eq(message));
        }
        self.manager
            .borrow_mut()
            .mailbox
            .insert(self.user_id, Vec::default());
        false
    }

    pub fn take_message<T: Into<Log>>(&self, log: T) -> MessageReplier {
        MessageReplier::new(self.remove_message(log), self.manager)
    }

    pub fn reply(&self, log: Log, payload: impl Encode, value: u128) -> RunResult {
        self.reply_bytes(log, payload.encode(), value)
    }

    pub fn reply_bytes(&self, log: Log, raw_payload: impl AsRef<[u8]>, value: u128) -> RunResult {
        self.take_message(log).reply_bytes(raw_payload, value)
    }

    pub fn claim_value<T: Into<Log>>(&self, log: T) {
        let message = self.remove_message(log);
        self.manager.borrow_mut().send_value(
            message.source(),
            Some(message.destination()),
            message.value(),
        );
    }

    fn remove_message<T: Into<Log>>(&self, log: T) -> StoredMessage {
        let log = log.into();
        let mut manager = self.manager.borrow_mut();
        let messages = manager
            .mailbox
            .get_mut(&self.user_id)
            .expect("Infallible. No mailbox associated with this user id");
        let index = messages
            .iter()
            .position(|message| log.eq(message))
            .expect("No message that satisfies log");
        messages.remove(index)
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
            raw_payload.as_ref().to_vec().try_into().unwrap(),
            None,
            value,
            self.log
                .status_code()
                .map(|status_code| ReplyDetails::new(self.log.id(), status_code).into()),
        );

        self.manager
            .borrow_mut()
            .validate_and_run_dispatch(Dispatch::new(DispatchKind::Reply, message))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use crate::{program::ProgramIdWrapper, Log, Program, System};
    use codec::Encode;
    use gear_core::{
        ids::MessageId,
        message::{Dispatch, DispatchKind, Message, Payload},
    };

    #[test]
    fn mailbox_walk_through_test() {
        //Arranging data for future messages
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let encoded_message_payload: Payload = message_payload.encode().try_into().unwrap();
        let reply_payload: Payload = vec![3, 2, 1].try_into().unwrap();
        let encoded_reply_payload: Payload = reply_payload.encode().try_into().unwrap();
        let log = Log::builder().payload(message_payload);

        //Building message based on arranged data
        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            encoded_message_payload.clone(),
            Default::default(),
            0,
            None,
        );

        //Sending created message and extracting its log
        let message_result =
            system.send_dispatch(Dispatch::new(DispatchKind::Handle, message.clone()));
        let message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        //Getting mailbox of destination user and extracting message
        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        let message_replier = destination_user_mailbox.take_message(log);

        //Replying on sended message and extracting log
        let reply_log = message_replier.reply(reply_payload, 0).log;
        let last_reply_log = reply_log.last().expect("No message log in run result");

        //Sending one more message to be sure that no critical move semantic didn't
        // occur
        let second_message_result =
            system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));
        let second_message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        //Asserting results
        assert!(!message_result.main_failed);
        assert!(!message_result.others_failed);
        assert!(!second_message_result.main_failed);
        assert!(!second_message_result.others_failed);
        assert_eq!(reply_log.len(), 1);
        assert_eq!(last_reply_log.payload(), encoded_reply_payload.get());
        assert_eq!(message_log.payload(), encoded_message_payload.get());
        assert_eq!(second_message_log.payload(), encoded_message_payload.get());
    }

    #[test]
    fn mailbox_deletes_message_after_reply() {
        //Arranging data for future messages
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let reply_payload: Payload = vec![3, 2, 1].try_into().unwrap();
        let message_log = Log::builder().payload(message_payload.clone());

        //Building message based on arranged data
        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().try_into().unwrap(),
            Default::default(),
            0,
            None,
        );

        //Sending created message
        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        //Getting mailbox of destination user and replying on it
        let mut destination_user_mailbox = system.get_mailbox(destination_user_id);
        destination_user_mailbox.reply(message_log.clone(), reply_payload, 0);

        //Making sure that original message deletes after reply
        destination_user_mailbox = system.get_mailbox(destination_user_id);
        assert!(!destination_user_mailbox.contains(&message_log))
    }

    #[test]
    fn mailbox_reply_bytes_test() {
        //Arranging data for future messages
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let reply_payload_array: [u8; 3] = [3, 2, 1];
        let reply_payload: Payload = reply_payload_array.to_vec().try_into().unwrap();
        let log = Log::builder().payload(message_payload.clone());

        //Building message based on arranged data
        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().try_into().unwrap(),
            Default::default(),
            0,
            None,
        );

        //Sending created message
        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        //Getting mailbox of destination user and extracting message
        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        let message_replier = destination_user_mailbox.take_message(log);

        //Replying by bytes and extracting result log
        let result = message_replier.reply_bytes(reply_payload_array, 0);
        let result_log = result.log;
        let last_result_log = result_log.last().expect("No message log in run result");

        assert_eq!(last_result_log.payload(), reply_payload.get());
    }

    #[test]
    fn mailbox_deletes_message_after_taking() {
        //Arranging data for future messages
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let log = Log::builder().payload(message_payload.clone());

        //Building message based on arranged data
        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().try_into().unwrap(),
            Default::default(),
            0,
            None,
        );

        //Sending created message
        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        //Getting mailbox of destination user and extracting message
        let destination_user_mailbox = system.get_mailbox(destination_user_id);
        destination_user_mailbox.take_message(log.clone());

        //Making sure that taken message is deleted
        assert!(!destination_user_mailbox.contains(&log))
    }

    #[test]
    #[should_panic(expected = "No message that satisfies log")]
    fn take_unknown_log_message() {
        // Arranging data for future messages
        let system = System::new();
        let source_user_id = 100;
        let destination_user_id = 200;
        let log = Log::builder().source(source_user_id);

        // Taking mailbox and message that doesn't exists
        let mailbox = system.get_mailbox(destination_user_id);
        mailbox.take_message(log);
    }

    #[test]
    #[should_panic(expected = "Mailbox available only for users")]
    fn take_programs_mailbox() {
        // Setting up variables for test
        let system = System::new();
        let restricted_user_id = 42;
        Program::from_file_with_id(
            &system,
            restricted_user_id,
            "../target/wasm32-unknown-unknown/release/demo_futures_unordered.wasm",
        );

        // Getting user id that is already registered as a program
        system.get_mailbox(restricted_user_id);
    }

    #[test]
    fn claim_value_from_mailbox() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let sender_id = 1;
        let receiver_id = 42;
        let payload = b"hello".to_vec();

        let log = Log::builder()
            .source(sender_id)
            .dest(receiver_id)
            .payload(payload.clone());

        let message = Message::new(
            message_id,
            sender_id.into(),
            receiver_id.into(),
            payload.encode().try_into().unwrap(),
            Default::default(),
            1000,
            None,
        );

        system.mint_to(sender_id, 1000);
        system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

        let receiver_mailbox = system.get_mailbox(receiver_id);
        receiver_mailbox.claim_value(log);

        assert_eq!(system.balance_of(receiver_id), 1000);
    }

    #[test]
    fn delayed_dispatches_works() {
        // Arranging data for future messages.
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let log = Log::builder().payload(message_payload.clone());

        // Building message based on arranged data.
        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.encode().try_into().unwrap(),
            Default::default(),
            0,
            None,
        );

        let bn_before_schedule = 5;
        let scheduled_delay = 10;
        system.0.borrow_mut().send_delayed_dispatch(
            Dispatch::new(DispatchKind::Handle, message),
            scheduled_delay,
        );

        let mailbox = system.get_mailbox(destination_user_id);
        assert!(!mailbox.contains(&log));

        // Run to block number before scheduled delay
        assert_eq!(system.spend_blocks(bn_before_schedule).len(), 0);
        assert!(!mailbox.contains(&log));

        // Run to block number at scheduled delay
        assert_eq!(
            system
                .spend_blocks(scheduled_delay - bn_before_schedule)
                .len(),
            1
        );
        assert!(mailbox.contains(&log));
    }
}
