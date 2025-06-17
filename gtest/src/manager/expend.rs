// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use gear_common::gas_provider::Imbalance;

impl ExtManager {
    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub(crate) fn spend_gas(&mut self, id: MessageId, amount: u64) {
        if amount.is_zero() {
            return;
        }

        self.gas_tree.spend(id, amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed spending gas. Message id - {id}, amount - {amount}. Got error - {e:?}"
            );

            unreachable!("{err_msg}");
        });

        let (external, multiplier, _) = self.gas_tree.get_origin_node(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed getting origin node for the current one. Message id - {id}, Got error - {e:?}"
            );
            unreachable!("{err_msg}");
        });

        self.bank.spend_gas(external.cast(), amount, multiplier)
    }

    pub(crate) fn spend_burned(&mut self, id: MessageId, amount: u64) {
        self.gas_burned
            .entry(id)
            .and_modify(|v| *v = v.saturating_sub(amount))
            .or_insert(amount);
        self.spend_gas(id, amount);
    }

    pub(crate) fn cost_by_storage_type(storage_type: StorageType) -> u64 {
        // Cost per block based on the storage used for holding
        let schedule = Schedule::default();
        let RentWeights {
            waitlist,
            dispatch_stash,
            reservation,
            mailbox,
            ..
        } = schedule.rent_weights;
        match storage_type {
            StorageType::Code => todo!("#646"),
            StorageType::Waitlist => waitlist.ref_time,
            StorageType::Mailbox => mailbox.ref_time,
            StorageType::DispatchStash => dispatch_stash.ref_time,
            StorageType::Program => todo!("#646"),
            StorageType::Reservation => reservation.ref_time,
        }
    }

    pub(crate) fn consume_and_retrieve(&mut self, id: impl Origin) {
        let id_origin = id.into_origin();
        let outcome = self.gas_tree.consume(id_origin).unwrap_or_else(|e| {
            let err_msg = format!(
                "consume_and_retrieve: failed consuming the rest of gas. Got error - {e:?}"
            );

            unreachable!("{err_msg}")
        });

        if let Some((imbalance, multiplier, external)) = outcome {
            let gas_left = imbalance.peek();
            log::debug!(
                "Consumed message {id_origin}. Unreserving {gas_left} (gas) from {external:?}",
            );

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
