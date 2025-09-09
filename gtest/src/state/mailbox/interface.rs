// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use super::manager::{MailboxErrorImpl, MailboxedMessage};
use crate::{
    MAX_USER_GAS_LIMIT, UserMessageEvent, Value,
    constants::BlockNumber,
    error::usage_panic,
    manager::ExtManager,
    state::{accounts::Accounts, programs::ProgramsStorageManager},
};
use gear_common::storage::Interval;
use gear_core::{
    ids::{ActorId, MessageId, prelude::MessageIdExt as _},
    message::{ReplyMessage, ReplyPacket},
};
use parity_scale_codec::Encode;
use std::cell::RefCell;

/// Interface to a particular user mailbox.
///
/// Gives a simplified interface to perform some operations
/// over a particular user mailbox.
pub struct UserMailbox<'a> {
    manager: &'a RefCell<ExtManager>,
    user_id: ActorId,
}

impl<'a> UserMailbox<'a> {
    pub(crate) fn new(user_id: ActorId, manager: &'a RefCell<ExtManager>) -> UserMailbox<'a> {
        UserMailbox { user_id, manager }
    }

    /// Checks whether message with some traits (defined in `event`) is
    /// in mailbox.
    pub fn contains<T: Into<UserMessageEvent> + Clone>(&self, event: &T) -> bool {
        self.find_message_by_event(&event.clone().into()).is_some()
    }

    /// Sends user reply message.
    ///
    /// Same as [`Self::reply_bytes`], but payload is encoded
    /// in a *parity-scale-codec* format.
    pub fn reply(
        &self,
        event: impl Into<UserMessageEvent>,
        payload: impl Encode,
        value: Value,
    ) -> Result<MessageId, MailboxErrorImpl> {
        self.reply_bytes(event, payload.encode(), value)
    }

    /// Sends user reply message to a mailboxed message
    /// finding it in the mailbox by traits of `event`.
    pub fn reply_bytes(
        &self,
        event: impl Into<UserMessageEvent>,
        raw_payload: impl AsRef<[u8]>,
        value: Value,
    ) -> Result<MessageId, MailboxErrorImpl> {
        let reply_to_id = self
            .find_message_by_event(&event.into())
            .ok_or(MailboxErrorImpl::ElementNotFound)?
            .id();

        let mailboxed = self
            .manager
            .borrow_mut()
            .read_mailbox_message(self.user_id, reply_to_id)?;

        let destination = mailboxed.source();

        // No need to check the mailbox message source being builtin actor,
        // because builtins only send replies, which goes only to events storage.
        assert!(!self.manager.borrow().is_builtin(destination));

        let reply_id = MessageId::generate_reply(mailboxed.id());

        // Set zero gas limit if reply deposit exists.
        let gas_limit = if self
            .manager
            .borrow_mut()
            .gas_tree
            .exists_and_deposit(reply_id)
        {
            0
        } else {
            MAX_USER_GAS_LIMIT
        };

        // Build a reply message
        let dispatch = {
            let payload = raw_payload
                .as_ref()
                .to_vec()
                .try_into()
                .unwrap_or_else(|err| unreachable!("Can't send reply with such payload: {err:?}"));

            let message = ReplyMessage::from_packet(
                reply_id,
                ReplyPacket::new_with_gas(payload, gas_limit, value),
            );

            message.into_dispatch(self.user_id, destination, mailboxed.id())
        };

        Ok(self
            .manager
            .borrow_mut()
            .validate_and_route_dispatch(dispatch))
    }

    /// Claims value from a message in mailbox.
    ///
    /// If message with traits defined in `event` is not found, an error is
    /// returned.
    pub fn claim_value<T: Into<UserMessageEvent>>(&self, event: T) -> Result<(), MailboxErrorImpl> {
        let message_id = self
            .find_message_by_event(&event.into())
            .ok_or(MailboxErrorImpl::ElementNotFound)?
            .id();

        // User must exist
        if !Accounts::exists(self.user_id) {
            usage_panic!(
                "User's {} balance is zero; mint value to it first.",
                self.user_id
            );
        }

        let mailboxed = self
            .manager
            .borrow_mut()
            .read_mailbox_message(self.user_id, message_id)?;

        let destination = mailboxed.source();

        // No need to check the mailbox message source being builtin actor,
        // because builtins only send replies, which goes only to events storage.
        assert!(!self.manager.borrow().is_builtin(destination));

        if ProgramsStorageManager::is_active_program(destination) {
            let message = ReplyMessage::auto(mailboxed.id());

            self.manager
                .borrow_mut()
                .gas_tree
                .create(self.user_id, message.id(), 0, true)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            let dispatch = message.into_stored_dispatch(self.user_id, destination, mailboxed.id());

            self.manager.borrow_mut().dispatches.push_back(dispatch);
        }

        Ok(())
    }

    fn find_message_by_event(&self, event: &UserMessageEvent) -> Option<MailboxedMessage> {
        self.get_user_mailbox()
            .find_map(|(msg, _)| event.eq(&msg).then_some(msg))
    }

    fn get_user_mailbox(
        &self,
    ) -> impl Iterator<Item = (MailboxedMessage, Interval<BlockNumber>)> + use<> {
        self.manager.borrow().mailbox.iter_key(self.user_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        DEFAULT_USER_ALICE, EXISTENTIAL_DEPOSIT, EventBuilder, GAS_MULTIPLIER, Program, System,
    };
    use demo_constructor::{Call, Calls, Scheme, WASM_BINARY};
    use gear_core::{gas_metering::RentWeights, ids::ActorId};
    use parity_scale_codec::Encode;

    fn prepare_program(system: &System) -> (Program<'_>, ([u8; 32], Vec<u8>, EventBuilder)) {
        let program = Program::from_binary_with_id(system, 121, WASM_BINARY);

        let sender = ActorId::from(DEFAULT_USER_ALICE).into_bytes();
        let payload = b"sup!".to_vec();
        let event_builder = EventBuilder::new()
            .with_destination(sender)
            .with_payload_bytes(payload.clone());

        let msg_id = program.send(sender, Scheme::empty());
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));

        (program, (sender, payload, event_builder))
    }

    #[test]
    fn claim_value_from_mailbox() {
        let system = System::new();
        let (program, (sender, payload, event_builder)) = prepare_program(&system);

        let original_balance = system.balance_of(sender);

        let value_send = 2 * EXISTENTIAL_DEPOSIT;
        let handle = Calls::builder().send_value(sender, payload, value_send);
        let msg_id = program.send_bytes_with_value(sender, handle.encode(), value_send);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
        assert!(res.contains(&event_builder));

        assert_eq!(
            system.balance_of(sender),
            original_balance
                - value_send
                - res.spent_value()
                - GAS_MULTIPLIER.gas_to_value(RentWeights::default().mailbox_threshold.ref_time)
        );

        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&event_builder));
        assert!(mailbox.claim_value(event_builder).is_ok());
        assert_eq!(
            system.balance_of(sender),
            original_balance - res.spent_value()
        );
    }

    #[test]
    fn reply_to_mailbox_message() {
        let system = System::new();
        let (program, (sender, payload, event_builder)) = prepare_program(&system);

        let handle = Calls::builder().send(sender, payload);
        let msg_id = program.send(sender, handle);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
        assert!(res.contains(&event_builder));

        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&event_builder));
        let msg_id = mailbox
            .reply(event_builder, Calls::default(), 0)
            .expect("sending reply failed: didn't find message in mailbox");
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
    }

    #[test]
    fn delayed_mailbox_message() {
        let system = System::new();
        let (program, (sender, payload, event_builder)) = prepare_program(&system);

        let delay = 5;
        let handle = Calls::builder().add_call(Call::Send(
            sender.into(),
            payload.into(),
            None,
            0.into(),
            delay.into(),
        ));
        let msg_id = program.send(sender, handle);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));

        let results = system.run_scheduled_tasks(delay);
        let delayed_dispatch_res = results.last().expect("internal error: no blocks spent");

        assert!(delayed_dispatch_res.contains(&event_builder));
        let mailbox = system.get_mailbox(sender);
        assert!(mailbox.contains(&event_builder));
    }
}
