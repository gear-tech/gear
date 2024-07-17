// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{manager::ExtManager, Log, RunResult, GAS_ALLOWANCE};
use codec::Encode;
use gear_common::{auxiliary::mailbox::*, storage::Interval};
use gear_core::{
    ids::{prelude::MessageIdExt, MessageId, ProgramId},
    message::{ReplyMessage, ReplyPacket},
};
use std::{cell::RefCell, convert::TryInto};

/// Interface to a particular user mailbox.
///
/// Gives a simplified interface to perform some operations
/// over a particular user mailbox.
pub struct ActorMailbox<'a> {
    manager: &'a RefCell<ExtManager>,
    user_id: ProgramId,
}

impl<'a> ActorMailbox<'a> {
    pub(crate) fn new(user_id: ProgramId, manager: &'a RefCell<ExtManager>) -> ActorMailbox<'a> {
        ActorMailbox { user_id, manager }
    }

    /// Checks whether message with some traits (defined in `log`) is
    /// in mailbox.
    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        self.find_message_by_log(&log.clone().into()).is_some()
    }

    /// Sends user reply message.
    ///
    /// Same as [`Self::reply_bytes`], but payload is encoded
    /// in a *partiy-scale-codec* format.
    pub fn reply(
        &self,
        log: Log,
        payload: impl Encode,
        value: u128,
    ) -> Result<RunResult, MailboxErrorImpl> {
        self.reply_bytes(log, payload.encode(), value)
    }

    /// Sends user reply message to a mailboxed message
    /// finding it in the mailbox by traits of `log`.
    pub fn reply_bytes(
        &self,
        log: Log,
        raw_payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<RunResult, MailboxErrorImpl> {
        let mailboxed_msg = self
            .find_message_by_log(&log)
            .ok_or(MailboxErrorImpl::ElementNotFound)?;
        self.manager
            .borrow()
            .mailbox
            .remove(self.user_id, mailboxed_msg.id())?;

        let dispatch = {
            let packet = ReplyPacket::new_with_gas(
                raw_payload
                    .as_ref()
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|err| panic!("Can't send reply with such payload: {err:?}")),
                GAS_ALLOWANCE,
                value,
            );
            let reply_message =
                ReplyMessage::from_packet(MessageId::generate_reply(mailboxed_msg.id()), packet);

            reply_message.into_dispatch(self.user_id, mailboxed_msg.source(), mailboxed_msg.id())
        };

        Ok(self
            .manager
            .borrow_mut()
            .validate_and_run_dispatch(dispatch))
    }

    /// Claims value from a message in mailbox.
    ///
    /// If message with traits defined in `log` is not found, an error is
    /// returned.
    pub fn claim_value<T: Into<Log>>(&self, log: T) -> Result<(), MailboxErrorImpl> {
        let mailboxed_msg = self
            .find_message_by_log(&log.into())
            .ok_or(MailboxErrorImpl::ElementNotFound)?;
        self.manager
            .borrow_mut()
            .claim_value_from_mailbox(self.user_id, mailboxed_msg.id())
            .unwrap_or_else(|e| unreachable!("Unexpected mailbox error: {e:?}"));

        Ok(())
    }

    fn find_message_by_log(&self, log: &Log) -> Option<MailboxedMessage> {
        self.get_user_mailbox()
            .find_map(|(msg, _)| log.eq(&msg).then_some(msg))
    }

    fn get_user_mailbox(&self) -> impl Iterator<Item = (MailboxedMessage, Interval<BlockNumber>)> {
        self.manager.borrow().mailbox.iter_key(self.user_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        program::ProgramIdWrapper, Log, Program, System, EXISTENTIAL_DEPOSIT, GAS_ALLOWANCE,
    };
    use codec::Encode;
    use demo_constructor::{Call, Calls, Scheme, WASM_BINARY};
    use gear_common::auxiliary::mailbox::MailboxErrorImpl;
    use gear_core::{
        ids::{prelude::MessageIdExt, MessageId, ProgramId},
        message::{Dispatch, DispatchKind, HandleMessage, HandlePacket, Payload},
    };
    use std::convert::TryInto;

    fn prepare_program(system: &System) -> (Program<'_>, ([u8; 32], Vec<u8>, Log)) {
        let program = Program::from_binary_with_id(system, 121, WASM_BINARY);

        let sender = ProgramId::from(42).into_bytes();
        let payload = b"sup!".to_vec();
        let log = Log::builder().dest(sender).payload_bytes(payload.clone());

        let res = program.send(sender, Scheme::empty());
        assert!(!res.main_failed());

        (program, (sender, payload, log))
    }

    #[test]
    fn user2user_doesnt_reach_mailbox() {
        let system = System::new();

        let source = ProgramIdWrapper::from(100).0;
        let message_id: MessageId = MessageId::generate_from_user(0, source, 0);
        let destination = ProgramIdWrapper::from(200).0;
        let payload: Payload = vec![1, 2, 3].try_into().expect("len exceed");
        let log = Log::builder()
            .dest(destination)
            .payload_bytes(payload.inner());

        let dispatch = Dispatch::new(
            DispatchKind::Handle,
            HandleMessage::from_packet(
                message_id,
                HandlePacket::new_with_gas(destination, payload, GAS_ALLOWANCE, 0),
            )
            .into_message(source),
        );

        // Log exists
        let res = system.send_dispatch(dispatch);
        assert!(res.contains(&log));

        // But message doesn't exist in mailbox
        let mailbox = system.get_mailbox(destination);
        let res = mailbox.reply(log, b"", 0);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), MailboxErrorImpl::ElementNotFound);
    }

    #[test]
    fn claim_value_from_mailbox() {
        let system = System::new();
        let (program, (sender, payload, log)) = prepare_program(&system);

        let original_balance = 20 * EXISTENTIAL_DEPOSIT;
        system.mint_to(sender, original_balance);

        let value_send = 2 * EXISTENTIAL_DEPOSIT;
        let handle = Calls::builder().send_value(sender, payload, value_send);
        let res = program.send_bytes_with_value(sender, handle.encode(), value_send);
        assert!(!res.main_failed());
        assert!(res.contains(&log));
        assert_eq!(system.balance_of(sender), original_balance - value_send);

        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&log));
        assert!(mailbox.claim_value(log).is_ok());
        assert_eq!(system.balance_of(sender), original_balance);
    }

    #[test]
    fn reply_to_mailbox_message() {
        let system = System::new();
        let (program, (sender, payload, log)) = prepare_program(&system);

        let handle = Calls::builder().send(sender, payload);
        let res = program.send(sender, handle);
        assert!(!res.main_failed());
        assert!(res.contains(&log));

        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&log));
        let res = mailbox
            .reply(log, Calls::default(), 0)
            .expect("sending reply failed: didn't find message in mailbox");
        assert!(!res.main_failed());
    }

    #[test]
    fn delayed_mailbox_message() {
        let system = System::new();
        let (program, (sender, payload, log)) = prepare_program(&system);

        let delay = 5;
        let handle = Calls::builder().add_call(Call::Send(
            sender.into(),
            payload.into(),
            None,
            0.into(),
            delay.into(),
        ));
        let res = program.send(sender, handle);
        assert!(!res.main_failed());

        let results = system.spend_blocks(delay);
        let delayed_dispatch_res = results.last().expect("internal error: no blocks spent");

        assert!(delayed_dispatch_res.contains(&log));
        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&log));
    }
}
