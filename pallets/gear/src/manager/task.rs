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

use crate::{manager::ExtManager, Config, Event, GasHandlerOf, Pallet, QueueOf};
use alloc::string::ToString;
use codec::Encode;
use common::{
    event::{MessageWokenSystemReason, SystemReason},
    scheduler::*,
    storage::*,
    GasTree, Origin,
};
use core_processor::common::{ExecutionErrorReason, JournalHandler};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    message::ReplyMessage,
};

impl<T: Config> TaskHandler<T::AccountId> for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn pause_program(&mut self, _program_id: ProgramId) {
        todo!("#646");
    }

    fn remove_code(&mut self, _code_id: CodeId) {
        todo!("#646");
    }

    fn remove_from_mailbox(&mut self, _user_id: T::AccountId, _message_id: MessageId) {
        todo!("#646");
    }

    // TODO: generate system signal for program (#647).
    fn remove_from_waitlist(&mut self, program_id: ProgramId, message_id: MessageId) {
        // Taking message from waitlist and charging for holding there.
        //
        // It's guaranteed to be addressed to program
        // or waitlist/scheduler storage invalidated!
        //
        // Note:
        // `assert_eq!(waitlisted.id(), message_id)`
        // `assert_eq!(waitlisted.destination(), program_id)`
        let waitlisted = self
            .wake_message_impl(program_id, message_id)
            .unwrap_or_else(|| unreachable!("Scheduling logic invalidated!"));

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::MessageWoken {
            id: waitlisted.id(),
            reason: MessageWokenSystemReason::OutOfRent.into_reason(),
        });

        // Trap explanation.
        let trap = ExecutionErrorReason::OutOfRent;

        // Generate trap reply.
        if self.check_program_id(&waitlisted.source()) {
            // Sending trap reply to program, by enqueuing it to message queue.
            let trap = trap.encode();

            // Creating reply message.
            let trap_reply = ReplyMessage::system(message_id, trap, core_processor::ERR_EXIT_CODE)
                .into_stored_dispatch(program_id, waitlisted.source(), message_id);

            // Splitting gas for newly created reply message.
            // TODO: handle error case for `split` (#1130).
            let _ = GasHandlerOf::<T>::split(trap_reply.id(), message_id);

            // Enqueueing dispatch into message queue.
            QueueOf::<T>::queue(trap_reply)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            // Sending trap reply to user, by depositing event.

            // Note: for users, trap replies always contain
            // string explanation of the error.
            let trap = trap.to_string().into_bytes();

            // Creating reply message.
            let trap_reply = ReplyMessage::system(message_id, trap, core_processor::ERR_EXIT_CODE)
                .into_stored(program_id, waitlisted.source(), message_id);

            // Depositing appropriate event.
            Pallet::<T>::deposit_event(Event::UserMessageSent {
                message: trap_reply,
                expiration: None,
            });
        }

        // Consuming gas handler for waitlisted message.
        self.message_consumed(waitlisted.id());
    }

    fn remove_paused_program(&mut self, _program_id: ProgramId) {
        todo!("#646");
    }

    fn wake_message(&mut self, _program_id: ProgramId, _message_id: MessageId) {
        todo!("issue #349");
    }
}
