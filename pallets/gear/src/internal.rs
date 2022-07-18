// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Internal details of Gear Pallet implementation.

use crate::{
    Authorship, BalanceOf, Config, CostsPerBlockOf, CurrencyOf, Event, GasHandlerOf, Pallet,
    SchedulingCostOf, SystemPallet, TaskPoolOf, WaitlistOf,
};
use common::{
    event::{MessageWaitedReason, MessageWokenReason},
    scheduler::*,
    storage::*,
    GasPrice, GasProvider, GasTree, Origin,
};
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Imbalance, ReservableCurrency,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::StoredDispatch,
};
use sp_runtime::traits::{Saturating, UniqueSaturatedInto, Zero};

pub(crate) struct Deadline<T: Config> {
    pub(crate) schedule_at: BlockNumberFor<T>,
    #[allow(unused)]
    pub(crate) gas_lock: <T::GasProvider as GasProvider>::Balance,
}

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // TODO: Consider usage of `Balance` instead of gas conversions.
    // TODO: Consider using here some tolerance. Missed due to identity fee.
    // TODO: If tolerance applied, consider unreserve excess funds, while
    // converting gas into value.
    /// Moves reserved funds from account to freed funds of another account.
    ///
    /// Repatriates reserved if both accounts exist,
    /// uses direct transfer otherwise.
    pub(crate) fn transfer_reserved(from: &T::AccountId, to: &T::AccountId, value: BalanceOf<T>) {
        // If destination account can reserve minimum balance, it means that
        // account exists and can receive repatriation of reserved funds.
        //
        // Otherwise need to transfer them directly.

        // Querying minimum balance (existential deposit).
        let existential_deposit = CurrencyOf::<T>::minimum_balance();

        // Bool var to check if we need just unreserve in case of self transfer.
        let self_transfer = from == to;

        // Checking balance existence of destination address.
        if !self_transfer && CurrencyOf::<T>::can_reserve(to, existential_deposit) {
            // Repatriating reserved to existent account.
            let unrevealed =
                CurrencyOf::<T>::repatriate_reserved(from, to, value, BalanceStatus::Free)
                    .unwrap_or_else(|e| {
                        unreachable!("Failed to repatriate reserved funds: {:?}", e)
                    });

            // Validating unrevealed funds after repatriation.
            if !unrevealed.is_zero() {
                unreachable!("Reserved funds wasn't fully repatriated.")
            }
        } else {
            // Unreserving funds from sender to transfer them directly.
            let unrevealed = CurrencyOf::<T>::unreserve(from, value);

            // Validating unrevealed funds after unreserve.
            if !unrevealed.is_zero() {
                unreachable!("Not all requested value was unreserved");
            }

            // Transfer to inexistent account, if need.
            if !self_transfer {
                CurrencyOf::<T>::transfer(from, to, value, ExistenceRequirement::AllowDeath)
                    .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
            }
        }
    }

    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub(crate) fn spend_gas(
        message_id: MessageId,
        amount: <T::GasProvider as GasProvider>::Balance,
    ) {
        // Spending gas amount from `GasNode`.
        // Here is a negative imbalance. Used `_` to force drop in place.
        let _ = GasHandlerOf::<T>::spend(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying external id. Fails in cases of `GasTree` invalidations.
        let opt_external = GasHandlerOf::<T>::get_external(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // External id may not be found only for inexistent node.
        let external = opt_external.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

        // Querying actual block author to reward.
        let block_author = Authorship::<T>::author()
            .unwrap_or_else(|| unreachable!("Failed to find block author!"));

        // Converting gas amount into value.
        let value = T::GasPrice::gas_price(amount);

        // Transferring reserved funds from external to block author.
        Self::transfer_reserved(&external, &block_author, value);
    }

    /// Consumes message by given `MessageId`.
    ///
    /// Updates currency and balances data on imbalance creation.
    ///
    /// SAFETY NOTE: calls `unreachable!()` in cases of `GasHandler::consume`
    /// errors or on non-zero unrevealed balances in `Currency::unreserve`.
    pub(crate) fn consume_message(message_id: MessageId) {
        // Consuming `GasNode`, returning optional outcome with imbalance.
        let outcome = GasHandlerOf::<T>::consume(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Unreserving funds, if imbalance returned.
        if let Some((imbalance, external)) = outcome {
            // Peeking numeric value from negative imbalance.
            let gas_left = imbalance.peek();

            // Unreserving funds, if left non-zero amount of gas.
            if !gas_left.is_zero() {
                log::debug!("Unreserve on message consumed: {gas_left} to {external:?}");

                // Converting gas amount into value.
                let value = T::GasPrice::gas_price(gas_left);

                // Unreserving funds.
                let unrevealed = CurrencyOf::<T>::unreserve(&external, value);

                // Validating unrevealed funds after unreserve.
                if !unrevealed.is_zero() {
                    unreachable!("Not all requested value was unreserved");
                }
            }
        }
    }

    /// Calculates maximal deadline for given `MessageId` and cost.
    #[must_use]
    pub(crate) fn maximal_deadline(
        message_id: MessageId,
        cost: SchedulingCostOf<T>,
    ) -> Option<Deadline<T>> {
        // Querying gas limit. Fails in cases of `GasTree` invalidations.
        let opt_limit = GasHandlerOf::<T>::get_limit(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Gas limit may not be found only for inexistent node.
        let (limit, _) = opt_limit.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

        // Amount of blocks might be payed for holding with given cost.
        let maximal_duration: BlockNumberFor<T> =
            limit.saturating_div(cost).unique_saturated_into();

        // Default reserve/stock to make sure that we process task in time.
        let reserve = CostsPerBlockOf::<T>::reserve_for();

        // Safety duration (maximal subtracted by reserve).
        let safety_duration = maximal_duration.saturating_sub(reserve);

        // Calling inner implementation.
        Self::deadline_for(safety_duration, cost)
    }

    /// Calculates deadline at specific block for given `MessageId` and cost.
    #[allow(unused)]
    #[must_use]
    pub(crate) fn deadline_at(
        message_id: MessageId,
        cost: SchedulingCostOf<T>,
        at: BlockNumberFor<T>,
    ) -> Option<Deadline<T>> {
        // Current block number.
        let current = SystemPallet::<T>::block_number();

        // Expected safety duration.
        let safety_duration = at.saturating_sub(current);

        // Calling inner implementation.
        let deadline = Self::deadline_for(safety_duration, cost)?;

        // Querying gas limit. Fails in cases of `GasTree` invalidations.
        let opt_limit = GasHandlerOf::<T>::get_limit(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Gas limit may not be found only for inexistent node.
        let (limit, _) = opt_limit.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

        // Checking that message gas limit can cover required lock.
        (limit >= deadline.gas_lock).then(|| deadline)
    }

    // Calculates deadline for duration holding.
    fn deadline_for(
        safety_duration: BlockNumberFor<T>,
        cost: SchedulingCostOf<T>,
    ) -> Option<Deadline<T>> {
        // Checking if duration is zero.
        if safety_duration.is_zero() {
            return None;
        }

        // Current block number.
        let current = SystemPallet::<T>::block_number();

        // Expected block number for task to be processed.
        let schedule_at = current.saturating_add(safety_duration);

        // Default reserve/stock to make sure that we process task in time.
        let reserve = CostsPerBlockOf::<T>::reserve_for();

        // Maximal duration to be payed.
        let maximal_duration: u64 = schedule_at.saturating_add(reserve).unique_saturated_into();

        // Gas limit to lock for charging for maximal duration.
        let gas_lock = maximal_duration.saturating_mul(cost);

        // Aggregating data.
        Some(Deadline {
            schedule_at,
            gas_lock,
        })
    }

    /// Charges for holding in some storage.
    pub(crate) fn charge_for_hold(
        message_id: MessageId,
        held_since: BlockNumberFor<T>,
        cost: SchedulingCostOf<T>,
    ) {
        // Current block number.
        let current = SystemPallet::<T>::block_number();

        // Holding duration.
        let duration: u64 = current.saturating_sub(held_since).unique_saturated_into();

        // Amount of gas to charge for holding.
        let amount = duration.saturating_mul(cost);

        // Spending gas, if need.
        if !amount.is_zero() {
            // Spending gas.
            Self::spend_gas(message_id, amount)
        }
    }

    /// Adds dispatch into waitlist, deposits event and adds task for waking it.
    pub(crate) fn wait_dispatch(dispatch: StoredDispatch, reason: MessageWaitedReason) {
        // Figuring out maximal deadline of holding.
        if let Some(maximal_deadline) =
            Self::maximal_deadline(dispatch.id(), CostsPerBlockOf::<T>::waitlist())
        {
            // Querying origin message id. Fails in cases of `GasTree` invalidations.
            let opt_origin_msg = GasHandlerOf::<T>::get_origin_key(dispatch.id())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Gas origin message id may not be found only for inexistent node.
            let origin_msg =
                opt_origin_msg.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

            // TODO: lock funds for holding here.
            // Depositing appropriate event.
            Self::deposit_event(Event::MessageWaited {
                id: dispatch.id(),
                origin: origin_msg.ne(&dispatch.id()).then_some(origin_msg),
                expiration: maximal_deadline.schedule_at,
                reason,
            });

            // Adding wake request in task pool.
            TaskPoolOf::<T>::add(
                maximal_deadline.schedule_at,
                ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
            )
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

            // Adding message in waitlist.
            WaitlistOf::<T>::insert(dispatch, maximal_deadline.schedule_at)
                .unwrap_or_else(|e| unreachable!("Waitlist corrupted! {:?}", e));
        } else {
            // Corner case. Should be rechecked for unreachable usage.
            log::error!("Unable to figure out deadline for: {dispatch:?}");
        }
    }

    /// Wakes dispatch from waitlist, permanently charged for hold with
    /// appropriate event depositing, if found.
    pub(crate) fn wake_dispatch(
        program_id: ProgramId,
        message_id: MessageId,
        reason: MessageWokenReason,
    ) -> Option<StoredDispatch> {
        WaitlistOf::<T>::remove(program_id, message_id)
            .map(|v| Self::wake_requirements(v, reason))
            .ok()
    }

    /// Charges and deposits event for already taken from waitlist dispatch.
    pub(crate) fn wake_requirements(
        (waitlisted, held): (StoredDispatch, Interval<BlockNumberFor<T>>),
        reason: MessageWokenReason,
    ) -> StoredDispatch {
        // Charging for holding.
        Self::charge_for_hold(
            waitlisted.id(),
            held.since,
            CostsPerBlockOf::<T>::waitlist(),
        );

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::MessageWoken {
            id: waitlisted.id(),
            reason,
        });

        // Delete if exists.
        let _ = TaskPoolOf::<T>::delete(
            held.till,
            ScheduledTask::RemoveFromWaitlist(waitlisted.destination(), waitlisted.id()),
        );

        waitlisted
    }
}
