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

use alloc::collections::BTreeMap;

use crate::common::{JournalHandler, JournalNote};

pub fn handle_journal(
    journal: impl IntoIterator<Item = JournalNote>,
    handler: &mut dyn JournalHandler,
) {
    let mut page_updates = BTreeMap::new();
    let mut nonces = BTreeMap::new();

    for note in journal.into_iter() {
        match note {
            JournalNote::ExecutionFail {
                origin,
                program_id,
                reason,
            } => handler.execution_fail(origin, program_id, reason),
            JournalNote::GasBurned { origin, amount } => handler.gas_burned(origin, amount),
            JournalNote::MessageConsumed(message_id) => handler.message_consumed(message_id),
            JournalNote::SendMessage { origin, message } => handler.send_message(origin, message),
            JournalNote::SubmitProgram { owner, program } => handler.submit_program(owner, program),
            JournalNote::WaitDispatch(dispatch) => handler.wait_dispatch(dispatch),
            JournalNote::WakeMessage { origin, message_id } => {
                handler.wake_message(origin, message_id)
            }
            JournalNote::UpdateNonce {
                origin: _origin,
                program_id,
                nonce,
            } => {
                let _ = nonces.insert(program_id, nonce);
            }
            JournalNote::UpdatePage {
                origin: _origin,
                program_id,
                page_number,
                data,
            } => {
                let entry = page_updates.entry(program_id).or_insert_with(BTreeMap::new);
                let _ = entry.insert(page_number, data);
            }
        }
    }

    for (program_id, pages) in page_updates {
        for (page_number, data) in pages {
            handler.update_page(program_id, page_number, data);
        }
    }

    for (program_id, nonce) in nonces {
        handler.update_nonce(program_id, nonce);
    }
}
