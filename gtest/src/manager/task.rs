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

/// Implementation of the `TaskHandler` trait for the `ExtManager`.
use super::ExtManager;
use gear_common::{
    scheduler::{StorageType, TaskHandler},
    Gas as GearCommonGas,
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::ReplyMessage,
};

impl TaskHandler<ProgramId> for ExtManager {
    fn pause_program(&mut self, _program_id: ProgramId) -> GearCommonGas {
        log::debug!("Program rent logic is disabled.");

        0
    }

    fn remove_code(&mut self, _code_id: CodeId) -> GearCommonGas {
        todo!("#646")
    }

    fn remove_from_mailbox(&mut self, user_id: ProgramId, message_id: MessageId) -> GearCommonGas {
        let message = ReplyMessage::auto(message_id);

        if !self.gas_tree.exists_and_deposit(message.id()) {
            self.gas_tree
                .create(user_id, message.id(), 0)
                .expect("failed to create gas tree node");
        }

        let mailboxed = self
            .claim_value_from_mailbox(user_id, message_id)
            .expect("failed to claim value from mailbox");

        let dispatch = message.into_stored_dispatch(
            mailboxed.destination(),
            mailboxed.source(),
            mailboxed.id(),
        );

        self.dispatches.push_back(dispatch);

        GearCommonGas::MIN
    }

    fn remove_from_waitlist(
        &mut self,
        _program_id: ProgramId,
        _message_id: MessageId,
    ) -> GearCommonGas {
        todo!()
    }

    fn remove_paused_program(&mut self, _program_id: ProgramId) -> GearCommonGas {
        todo!("#646")
    }

    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> GearCommonGas {
        let (dispatch, _) = self
            .wait_list
            .remove(&(program_id, message_id))
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));
        self.dispatches.push_back(dispatch);

        GearCommonGas::MIN
    }

    fn send_dispatch(&mut self, stashed_message_id: MessageId) -> GearCommonGas {
        let (dispatch, hold_interval) = self
            .dispatches_stash
            .remove(&stashed_message_id)
            .unwrap_or_else(|| unreachable!("TaskPool corrupted"));

        self.charge_for_hold(dispatch.id(), hold_interval, StorageType::DispatchStash);

        self.dispatches.push_back(dispatch.into());
        GearCommonGas::MIN
    }

    fn send_user_message(
        &mut self,
        stashed_message_id: MessageId,
        to_mailbox: bool,
    ) -> GearCommonGas {
        let (message, hold_interval) = self
            .dispatches_stash
            .remove(&stashed_message_id)
            .map(|(dispatch, interval)| (dispatch.into_parts().1, interval))
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));

        self.charge_for_hold(message.id(), hold_interval, StorageType::DispatchStash);

        let mailbox_message = message.clone().try_into().unwrap_or_else(|e| {
            unreachable!("invalid message: can't be converted to user message {e:?}")
        });

        self.send_user_message_after_delay(mailbox_message, to_mailbox);
        self.log.push(message);

        GearCommonGas::MIN
    }

    fn remove_gas_reservation(
        &mut self,
        _program_id: ProgramId,
        _reservation_id: ReservationId,
    ) -> GearCommonGas {
        todo!()
    }

    fn remove_resume_session(&mut self, _session_id: u32) -> GearCommonGas {
        log::debug!("Program rent logic is disabled");
        0
    }
}
