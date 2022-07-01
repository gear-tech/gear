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

use crate::{manager::ExtManager, Config};
use common::{scheduler::*, Origin};
use gear_core::ids::{CodeId, MessageId, ProgramId};

impl<T: Config> TaskHandler<T::AccountId> for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn pause_program(&mut self, _program_id: ProgramId) {
        todo!("issue #646");
    }

    fn remove_code(&mut self, _code_id: CodeId) {
        todo!("issue #646");
    }

    fn remove_from_mailbox(&mut self, _user_id: T::AccountId, _message_id: MessageId) {
        todo!("issue #646");
    }

    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) {
        unimplemented!();
    }

    fn remove_paused_program(&mut self, _program_id: ProgramId) {
        todo!("issue #646");
    }

    fn wake_message(&mut self, _program_id: ProgramId, _message_id: MessageId) {
        todo!("issue #349");
    }
}
