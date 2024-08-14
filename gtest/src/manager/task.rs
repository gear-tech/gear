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
use gear_core::ids::{ProgramId, MessageId, CodeId, ReservationId};
use gear_common::{scheduler::TaskHandler, Gas as GearCommonGas};

impl TaskHandler<ProgramId> for ExtManager {
    fn pause_program(&mut self, _program_id: ProgramId) -> GearCommonGas {
        todo!()
    }

    fn remove_code(&mut self, _code_id: CodeId) -> GearCommonGas {
        todo!()
    }

    fn remove_from_mailbox(&mut self, _user_id: ProgramId, _message_id: MessageId) -> GearCommonGas {
        todo!()
    }

    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) -> GearCommonGas {
        todo!()
    }

    fn remove_paused_program(&mut self, _program_id: ProgramId) -> GearCommonGas {
        todo!()
    }

    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> GearCommonGas {
        let (dispatch, _) = self.wait_list.remove(&(program_id, message_id))
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));
        self.dispatches.push_back(dispatch);

        GearCommonGas::MIN
    }

    fn send_dispatch(&mut self, stashed_message_id: MessageId) -> GearCommonGas {
        let dispatch = self.dispatches_stash.remove(&stashed_message_id)
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));

        self.dispatches.push_back(dispatch.into_stored());

        GearCommonGas::MIN
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, _to_mailbox: bool) -> GearCommonGas {
        let dispatch = self.dispatches_stash.remove(&stashed_message_id)
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));
        let stored_message = dispatch.into_parts().1.into_stored();
        let mailbox_message = stored_message
            .clone()
            .try_into()
            .unwrap_or_else(|e| unreachable!("invalid message: can't be converted to user message {e:?}"));
        
        self.mailbox
            .insert(mailbox_message)
            .unwrap_or_else(|e| unreachable!("Mailbox corrupted! {:?}", e));
        self.log.push(stored_message);

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
        todo!()
    }
}
