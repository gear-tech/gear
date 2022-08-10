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

use crate::common::{JournalHandler, JournalNote};
use alloc::{collections::BTreeMap, vec};

/// Handle some journal records passing them to the journal handler.
pub fn handle_journal(
    journal: impl IntoIterator<Item = JournalNote>,
    handler: &mut impl JournalHandler,
) {
    let mut page_updates = BTreeMap::new();
    let mut exit_list = vec![];
    let mut allocations_update = BTreeMap::new();

    for note in journal {
        match note {
            JournalNote::MessageDispatched {
                message_id,
                source,
                outcome,
            } => handler.message_dispatched(message_id, source, outcome),
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
            JournalNote::UpdatePage {
                program_id,
                page_number,
                data,
            } => {
                let entry = page_updates.entry(program_id).or_insert_with(BTreeMap::new);
                entry.insert(page_number, data);
            }
            JournalNote::UpdateAllocations {
                program_id,
                allocations,
            } => {
                allocations_update.insert(program_id, allocations);
            }
            JournalNote::SendValue { from, to, value } => handler.send_value(from, to, value),
            JournalNote::StoreNewPrograms {
                code_hash,
                candidates,
            } => handler.store_new_programs(code_hash, candidates),
            JournalNote::StopProcessing {
                dispatch,
                gas_burned,
            } => handler.stop_processing(dispatch, gas_burned),
            JournalNote::UpdateGasReservations {
                message_id,
                program_id,
                gas_reservation_map,
            } => handler.update_gas_reservation(message_id, program_id, gas_reservation_map),
            /*JournalNote::UnreserveGas {
                message_id,
                reservation_id,
                amount,
            } => handler.unreserve_gas(message_id, reservation_id, amount),*/
        }
    }

    for (program_id, pages_data) in page_updates {
        handler.update_pages_data(program_id, pages_data);
    }

    for (program_id, allocations) in allocations_update {
        handler.update_allocations(program_id, allocations);
    }

    for (id_exited, value_destination) in exit_list {
        handler.exit_dispatch(id_exited, value_destination);
    }
}
