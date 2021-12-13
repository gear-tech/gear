// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use crate::common::{JournalHandler, JournalNote};

pub fn handle_journal(
    journal: impl IntoIterator<Item = JournalNote>,
    handler: &mut dyn JournalHandler,
) {
    for note in journal.into_iter() {
        match note {
            JournalNote::SendMessage { origin, message } => handler.send_message(origin, message),
            JournalNote::ExecutionFail {
                origin,
                program_id,
                reason,
            } => handler.execution_fail(origin, program_id, reason),
            JournalNote::WaitDispatch(dispatch) => handler.wait_dispatch(dispatch),
            JournalNote::MessageConsumed(message_id) => handler.message_consumed(message_id),
            JournalNote::NotProcessed(dispatches) => handler.not_processed(dispatches),
            JournalNote::GasBurned { origin, amount } => handler.gas_burned(origin, amount),
            JournalNote::WakeMessage { origin, message_id } => {
                handler.wake_message(origin, message_id)
            }
            JournalNote::UpdatePage {
                origin,
                program_id,
                page_number,
                data,
            } => handler.update_page(origin, program_id, page_number, data),
        }
    }
}
