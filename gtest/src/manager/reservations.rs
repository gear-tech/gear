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

//! Various reservation related methods for ExtManager

use super::ExtManager;
use crate::state::programs::{GTestProgram, ProgramsStorageManager};
use gear_common::{ActorId, Program, ReservationId, scheduler::StorageType, storage::Interval};
use gear_core::{reservation::GasReservationSlot, tasks::ScheduledTask};
use std::mem;

impl ExtManager {
    pub(crate) fn remove_gas_reservation_impl(
        &mut self,
        program_id: ActorId,
        reservation: ReservationId,
    ) -> GasReservationSlot {
        let slot = self.update_program(program_id, |active_program| {
            active_program.gas_reservation_map
                .remove(&reservation)
                .unwrap_or_else(|| {
                    let err_msg = format!("ExtManager::remove_gas_reservation_impl: failed removing gas reservation. \
                    Reservation {reservation} doesn't exist.");

                    unreachable!("{err_msg}")
                })
        }).unwrap_or_else(|| {
            unreachable!("failed to update program {program_id}")
        });

        self.remove_gas_reservation_slot(reservation, slot)
    }

    pub(crate) fn remove_gas_reservation_with_task(
        &mut self,
        program_id: ActorId,
        reservation: ReservationId,
    ) {
        let slot = self.remove_gas_reservation_impl(program_id, reservation);

        let _ = self
            .task_pool
            .delete(
                slot.finish,
                ScheduledTask::RemoveGasReservation(program_id, reservation),
            )
            .map(|_| {
                self.on_task_pool_change();
            });
    }

    pub(crate) fn remove_gas_reservation_slot(
        &mut self,
        reservation: ReservationId,
        slot: GasReservationSlot,
    ) -> GasReservationSlot {
        let interval = Interval {
            start: slot.start,
            finish: slot.finish,
        };

        self.charge_for_hold(reservation, interval, StorageType::Reservation);
        self.consume_and_retrieve(reservation);

        slot
    }

    pub(crate) fn remove_gas_reservation_map(&mut self, program_id: ActorId) {
        ProgramsStorageManager::modify_program(program_id, |gtest_program| {
            if let Some(GTestProgram::Default(Program::Active(active_program))) = gtest_program {
                for (reservation_id, slot) in mem::take(&mut active_program.gas_reservation_map) {
                    let slot = self.remove_gas_reservation_slot(reservation_id, slot);

                    let result = self.task_pool.delete(
                        slot.finish,
                        ScheduledTask::RemoveGasReservation(program_id, reservation_id),
                    );
                    log::debug!(
                        "remove_gas_reservation_map; program_id = {program_id:?}, result = {result:?}"
                    );
                }
            }
        });
    }
}
