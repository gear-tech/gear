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

//! Various reservation related methods for ExtManager

use gear_common::{
    auxiliary::BlockNumber,
    gas_provider::Imbalance,
    scheduler::{ScheduledTask, StorageType},
    storage::Interval,
    MessageId, Origin, ProgramId, ReservationId,
};
use gear_core::{pages::num_traits::Zero, reservation::GasReservationSlot};

use crate::RESERVE_FOR;

use super::ExtManager;

impl ExtManager {
    pub(crate) fn remove_gas_reservation_impl(
        &mut self,
        program_id: ProgramId,
        reservation: ReservationId,
    ) -> GasReservationSlot {
        let slot = self.update_genuine_program(program_id, |p| {
            p.gas_reservation_map
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
        program_id: ProgramId,
        reservation: ReservationId,
    ) {
        let slot = self.remove_gas_reservation_impl(program_id, reservation);

        let _ = self.task_pool.delete(
            slot.finish,
            ScheduledTask::RemoveGasReservation(program_id, reservation),
        );
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

    pub(crate) fn consume_and_retrieve(&mut self, id: impl Origin) {
        let outcome = self.gas_tree.consume(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "consume_and_retrieve: failed consuming the rest of gas. Got error - {e:?}"
            );

            unreachable!("{err_msg}")
        });

        if let Some((imbalance, multiplier, external)) = outcome {
            let gas_left = imbalance.peek();

            if !gas_left.is_zero() {
                self.bank
                    .withdraw_gas(external.cast(), gas_left, multiplier);
            }
        }
    }

    pub(crate) fn charge_for_hold(
        &mut self,
        id: impl Origin,
        hold_interval: Interval<BlockNumber>,
        storage_type: StorageType,
    ) {
        let id: MessageId = id.cast();
        let current = self.block_height();

        // Deadline of the task.
        let deadline = hold_interval.finish.saturating_add(RESERVE_FOR);

        // The block number, which was the last paid for hold.
        //
        // Outdated tasks can end up being store for free - this case has to be
        // controlled by a correct selection of the `ReserveFor` constant.
        let paid_until = current.min(deadline);

        // holding duration
        let duration: u64 = paid_until.saturating_sub(hold_interval.start).into();

        // Cost per block based on the storage used for holding
        let cost = Self::cost_by_storage_type(storage_type);

        let amount = storage_type.try_into().map_or_else(
            |_| duration.saturating_mul(cost),
            |lock_id| {
                let prepaid = self.gas_tree.unlock_all(id, lock_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "charge_for_hold: failed unlocking locked gas.
                        Got error - {e:?}"
                    );

                    unreachable!("{err_msg}");
                });

                prepaid.min(duration.saturating_mul(cost))
            },
        );

        if !amount.is_zero() {
            self.spend_gas(id, amount);
        }
    }
}
