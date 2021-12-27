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
use alloc::collections::BTreeMap;

/// Handle some journal records passing them to the journal handler.
pub fn handle_journal(
    journal: impl IntoIterator<Item = JournalNote>,
    handler: &mut dyn JournalHandler,
) {
    let mut page_updates = BTreeMap::new();
    let mut nonces = BTreeMap::new();

    for note in journal.into_iter() {
        match note {
            JournalNote::MessageDispatched(outcome) => handler.message_dispatched(outcome),
            JournalNote::GasBurned {
                message_id,
                origin,
                amount,
            } => handler.gas_burned(message_id, origin, amount),
            JournalNote::MessageConsumed(message_id) => handler.message_consumed(message_id),
            JournalNote::SendMessage {
                message_id,
                message,
            } => handler.send_message(message_id, message),
            JournalNote::WaitDispatch(dispatch) => handler.wait_dispatch(dispatch),
            JournalNote::WakeMessage {
                message_id,
                program_id,
                awakening_id,
            } => handler.wake_message(message_id, program_id, awakening_id),
            JournalNote::UpdateNonceAndPagesAmount {
                program_id,
                persistent_pages,
                nonce,
            } => {
                let _ = nonces.insert(program_id, (persistent_pages, nonce));
            }
            JournalNote::UpdatePage {
                program_id,
                page_number,
                data,
            } => {
                let entry = page_updates.entry(program_id).or_insert_with(BTreeMap::new);
                let _ = entry.insert(page_number, data);
            }
        }
    }

    for (program_id, v) in nonces {
        handler.update_nonce_and_pages_amount(program_id, v.0, v.1);
    }

    for (program_id, pages) in page_updates {
        for (page_number, data) in pages {
            handler.update_page(program_id, page_number, data);
        }
    }
}
