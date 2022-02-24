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
use alloc::{collections::BTreeMap, vec};

/// Handle some journal records passing them to the journal handler.
pub fn handle_journal(
    journal: impl IntoIterator<Item = JournalNote>,
    handler: &mut dyn JournalHandler,
) {
    let mut page_updates = BTreeMap::new();
    let mut nonces = BTreeMap::new();
    let mut exit_list = vec![];

    for note in journal.into_iter() {
        match note {
            JournalNote::MessageDispatched(outcome) => handler.message_dispatched(outcome),
            JournalNote::GasBurned { message_id, amount } => handler.gas_burned(message_id, amount),
            JournalNote::ExitDispatch {
                id_exited,
                value_destination,
            } => exit_list.push((id_exited, value_destination)),
            JournalNote::MessageConsumed(message_id) => handler.message_consumed(message_id),
            JournalNote::SendDispatch {
                message_id,
                dispatch,
            } => handler.send_dispatch(message_id, dispatch),
            JournalNote::WaitDispatch(dispatch) => handler.wait_dispatch(dispatch),
            JournalNote::WakeMessage {
                message_id,
                program_id,
                awakening_id,
            } => handler.wake_message(message_id, program_id, awakening_id),
            JournalNote::UpdateNonce { program_id, nonce } => {
                nonces.insert(program_id, nonce);
            }
            JournalNote::UpdatePage {
                program_id,
                page_number,
                data,
            } => {
                let entry = page_updates.entry(program_id).or_insert_with(BTreeMap::new);
                entry.insert(page_number, data);
            }
            JournalNote::SendValue { from, to, value } => handler.send_value(from, to, value),
            JournalNote::StoreNewPrograms {
                code_hash,
                candidates,
            } => handler.store_new_programs(code_hash, candidates),
        }
    }

    for (program_id, nonce) in nonces {
        handler.update_nonce(program_id, nonce);
    }

    for (program_id, pages) in page_updates {
        for (page_number, data) in pages {
            handler.update_page(program_id, page_number, data);
        }
    }

    for (id_exited, value_destination) in exit_list {
        handler.exit_dispatch(id_exited, value_destination);
    }
}
