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
    Authorship, BalanceOf, Config, CostsPerBlockOf, CurrencyOf, Event, GasBalanceOf, GasHandlerOf,
    MailboxOf, Pallet, SchedulingCostOf, SystemPallet, TaskPoolOf, WaitlistOf,
};
use codec::{Decode, Encode};
use common::{
    event::{
        MessageWaitedReason, MessageWokenReason, Reason, UserMessageReadReason,
        UserMessageReadRuntimeReason,
    },
    scheduler::*,
    storage::*,
    GasPrice, GasTree, Origin,
};
use core_processor::common::ExecutionErrorReason;
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Imbalance, ReservableCurrency,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Message, StoredDispatch, StoredMessage},
};
use sp_runtime::traits::{Get, One, SaturatedConversion, Saturating, UniqueSaturatedInto, Zero};

/// Cost builder for `HoldBound<T>`.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HoldBoundCost<T: Config>(SchedulingCostOf<T>);

#[allow(unused)]
impl<T: Config> HoldBoundCost<T> {
    /// Creates bound to specific given block number.
    pub fn at(self, expected: BlockNumberFor<T>) -> HoldBound<T> {
        HoldBound {
            cost: self.0,
            expected,
        }
    }

    /// Creates bound to specific given deadline block number.
    pub fn deadline(self, deadline: BlockNumberFor<T>) -> HoldBound<T> {
        let expected = deadline.saturating_sub(CostsPerBlockOf::<T>::reserve_for());

        self.at(expected)
    }

    /// Creates bound for given duration since current block.
    pub fn duration(self, duration: BlockNumberFor<T>) -> HoldBound<T> {
        let expected = SystemPallet::<T>::block_number().saturating_add(duration);

        self.at(expected)
    }

    /// Creates maximal available bound for given gas limit.
    pub fn maximum_for(self, gas: GasBalanceOf<T>) -> HoldBound<T> {
        let deadline_duration = gas
            .saturating_div(self.0.max(One::one()))
            .saturated_into::<BlockNumberFor<T>>();

        let deadline = SystemPallet::<T>::block_number().saturating_add(deadline_duration);

        self.deadline(deadline)
    }

    /// Creates maximal available bound for given message id,
    /// by querying it's gas limit.
    pub fn maximum_for_message(self, message_id: MessageId) -> HoldBound<T> {
        // Querying gas limit. Fails in cases of `GasTree` invalidations.
        let gas_limit = GasHandlerOf::<T>::get_limit(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        self.maximum_for(gas_limit)
    }

    // Zero-duration hold bound.
    pub fn zero(self) -> HoldBound<T> {
        self.at(SystemPallet::<T>::block_number())
    }
}

/// Hold bound, specifying cost of storing, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HoldBound<T: Config> {
    /// Cost of storing per block.
    cost: SchedulingCostOf<T>,
    /// Expected block number task to be processed.
    expected: BlockNumberFor<T>,
}

// `unused` allowed because some fns may be used in future, but clippy
// doesn't allow this due to `pub(crate)` visibility.
#[allow(unused)]
impl<T: Config> HoldBound<T> {
    /// Creates cost builder for hold bound.
    pub fn by(cost: SchedulingCostOf<T>) -> HoldBoundCost<T> {
        assert!(!cost.is_zero());
        HoldBoundCost(cost)
    }

    /// Returns cost of storing per block, related to current hold bound.
    pub fn cost(&self) -> SchedulingCostOf<T> {
        self.cost
    }

    /// Returns expected block number task to be processed.
    pub fn expected(&self) -> BlockNumberFor<T> {
        self.expected
    }

    /// Returns expected duration before task will be processed, since now.
    pub fn expected_duration(&self) -> BlockNumberFor<T> {
        self.expected
            .saturating_sub(SystemPallet::<T>::block_number())
    }

    /// Returns the deadline for tasks to be processed.
    ///
    /// This deadline is exactly sum of expected block number and `reserve_for`
    /// safety duration from task pool overflow within the single block.
    pub fn deadline(&self) -> BlockNumberFor<T> {
        self.expected
            .saturating_add(CostsPerBlockOf::<T>::reserve_for())
    }

    /// Returns deadline duration before task will be processed, since now.
    pub fn deadline_duration(&self) -> BlockNumberFor<T> {
        self.deadline()
            .saturating_sub(SystemPallet::<T>::block_number())
    }

    /// Returns amount of gas should be locked for rent of the hold afterward.
    pub fn lock(&self) -> GasBalanceOf<T> {
        self.deadline_duration()
            .saturated_into::<GasBalanceOf<T>>()
            .saturating_mul(self.cost())
    }
}

// Internal functionality implementation.
impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // TODO (issue #1239):
    // - Consider usage of `Balance` instead of gas conversions.
    // - Consider usage of some tolerance here. Missed due to identity fee.
    // - If tolerance applied, consider unreserve excess funds, while
    // converting gas into value.
    /// Moves reserved funds from account to freed funds of another account.
    pub(crate) fn transfer_reserved(from: &T::AccountId, to: &T::AccountId, value: BalanceOf<T>) {
        // If value is zero, nothing to do.
        if value.is_zero() {
            return;
        }

        // If destination account can reserve minimum balance, it means that
        // account exists and can receive repatriation of reserved funds.
        //
        // Otherwise need to transfer them directly.

        // Checking balance existence of destination address.
        if CurrencyOf::<T>::can_reserve(to, CurrencyOf::<T>::minimum_balance()) {
            // Repatriating reserved to existent account.
            let unrevealed =
                CurrencyOf::<T>::repatriate_reserved(from, to, value, BalanceStatus::Free)
                    .unwrap_or_else(|e| {
                        unreachable!("Failed to repatriate reserved funds: {:?}", e)
                    });

            // TODO: Remove this once substrate bugfix PR merged
            // (https://github.com/paritytech/substrate/pull/11875).
            let unrevealed = (from != to)
                .then_some(unrevealed)
                .unwrap_or_else(|| value.saturating_sub(unrevealed));

            // Validating unrevealed funds after repatriation.
            if !unrevealed.is_zero() {
                unreachable!("Reserved funds wasn't fully repatriated: {:?}", unrevealed)
            }
        } else {
            // Unreserving funds from sender to transfer them directly.
            let unrevealed = CurrencyOf::<T>::unreserve(from, value);

            // Validating unrevealed funds after unreserve.
            if !unrevealed.is_zero() {
                unreachable!("Not all requested value was unreserved");
            }

            // Transfer to inexistent account.
            CurrencyOf::<T>::transfer(from, to, value, ExistenceRequirement::AllowDeath)
                .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
        }
    }

    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub(crate) fn spend_gas(message_id: MessageId, amount: GasBalanceOf<T>) {
        // If amount is zero, nothing to do.
        if amount.is_zero() {
            return;
        }

        // Spending gas amount from `GasNode`.
        GasHandlerOf::<T>::spend(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying external id. Fails in cases of `GasTree` invalidations.
        let external = GasHandlerOf::<T>::get_external(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying actual block author to reward.
        let block_author = Authorship::<T>::author()
            .unwrap_or_else(|| unreachable!("Failed to find block author!"));

        // Converting gas amount into value.
        let value = T::GasPrice::gas_price(amount);

        // Transferring reserved funds from external user to block author.
        Self::transfer_reserved(&external, &block_author, value);
    }

    /// Consumes message by given `MessageId`.
    ///
    /// Updates currency and balances data on imbalance creation.
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
                log::debug!(
                    "Message consumed. Unreserving {} from {:?}",
                    gas_left,
                    external
                );

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
        let hold =
            HoldBound::<T>::by(CostsPerBlockOf::<T>::waitlist()).maximum_for_message(dispatch.id());

        // Validating duration.
        if hold.expected_duration().is_zero() {
            // TODO: Replace with unreachable call after:
            // - `HoldBound` safety usage stabilized;
            // - Issue #1173 solved.
            log::error!("Failed to figure out correct wait hold bound");
            return;
        }

        // Querying origin message id. Fails in cases of `GasTree` invalidations.
        let origin_msg = GasHandlerOf::<T>::get_origin_key(dispatch.id())
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // TODO: Lock funds for holding here (issue #1173).

        // Depositing appropriate event.
        Self::deposit_event(Event::MessageWaited {
            id: dispatch.id(),
            origin: origin_msg.ne(&dispatch.id()).then_some(origin_msg),
            expiration: hold.expected(),
            reason,
        });

        // Adding wake request in task pool.
        TaskPoolOf::<T>::add(
            hold.expected(),
            ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
        )
        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

        // Adding message in waitlist.
        WaitlistOf::<T>::insert(dispatch, hold.expected())
            .unwrap_or_else(|e| unreachable!("Waitlist corrupted! {:?}", e));
    }

    /// Wakes dispatch from waitlist, permanently charged for hold with
    /// appropriate event depositing, if found.
    pub(crate) fn wake_dispatch(
        program_id: ProgramId,
        message_id: MessageId,
        reason: MessageWokenReason,
    ) -> Option<StoredDispatch> {
        // Removing dispatch from waitlist, doing wake requirements if found.
        WaitlistOf::<T>::remove(program_id, message_id)
            .map(|v| Self::wake_dispatch_requirements(v, reason))
            .ok()
    }

    /// Charges and deposits event for already taken from waitlist dispatch.
    pub(crate) fn wake_dispatch_requirements(
        (waitlisted, hold_interval): (StoredDispatch, Interval<BlockNumberFor<T>>),
        reason: MessageWokenReason,
    ) -> StoredDispatch {
        // Charging for holding.
        Self::charge_for_hold(
            waitlisted.id(),
            hold_interval.start,
            CostsPerBlockOf::<T>::waitlist(),
        );

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::MessageWoken {
            id: waitlisted.id(),
            reason,
        });

        // Delete task, if exists.
        let _ = TaskPoolOf::<T>::delete(
            hold_interval.finish,
            ScheduledTask::RemoveFromWaitlist(waitlisted.destination(), waitlisted.id()),
        );

        waitlisted
    }

    /// Removes message from mailbox, permanently charged for hold with
    /// appropriate event depositing, if found.
    ///
    /// Note: message auto-consumes, if reason is claim or reply.
    pub(crate) fn read_message(
        user_id: T::AccountId,
        message_id: MessageId,
        reason: UserMessageReadReason,
    ) -> Option<StoredMessage> {
        // Removing message from mailbox, doing read requirements if found.
        MailboxOf::<T>::remove(user_id, message_id)
            .map(|v| Self::read_message_requirements(v, reason))
            .ok()
    }

    /// Charges and deposits event for already taken from mailbox message.
    ///
    /// Note: message auto-consumes, if reason is claim or reply.
    pub(crate) fn read_message_requirements(
        (mailboxed, hold_interval): (StoredMessage, Interval<BlockNumberFor<T>>),
        reason: UserMessageReadReason,
    ) -> StoredMessage {
        use UserMessageReadRuntimeReason::{MessageClaimed, MessageReplied};

        // Charging for holding.
        Self::charge_for_hold(
            mailboxed.id(),
            hold_interval.start,
            CostsPerBlockOf::<T>::mailbox(),
        );

        // Determining if the reason is user action.
        let user_queries = matches!(reason, Reason::Runtime(MessageClaimed | MessageReplied));

        // Optionally consuming message.
        user_queries.then(|| Self::consume_message(mailboxed.id()));

        // Taking data for funds transfer.
        let user_id = <T::AccountId as Origin>::from_origin(mailboxed.destination().into_origin());
        let from = <T::AccountId as Origin>::from_origin(mailboxed.source().into_origin());
        let value = mailboxed.value().unique_saturated_into();

        // Determining recipients id.
        //
        // If message was claimed or replied, destination user takes value,
        // otherwise, it returns back (got unreserved).
        let to = if user_queries { &user_id } else { &from };

        // Transferring reserved funds, associated with the message.
        Self::transfer_reserved(&from, to, value);

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::UserMessageRead {
            id: mailboxed.id(),
            reason,
        });

        // Delete task, if exists.
        let _ = TaskPoolOf::<T>::delete(
            hold_interval.finish,
            ScheduledTask::RemoveFromMailbox(user_id, mailboxed.id()),
        );

        mailboxed
    }

    /// Sends message to user.
    ///
    /// It may be added to mailbox, if apply requirements.
    pub(crate) fn send_user_message(origin_msg: MessageId, message: Message) {
        // Querying `MailboxThreshold`, that represents minimal amount of gas
        // for message to be added to mailbox.
        let threshold = T::MailboxThreshold::get();

        // Figuring out gas limit for insertion.
        //
        // In case of sending with gas, we use applied gas limit, otherwise
        // finding available funds and trying to take threshold from them.
        let gas_limit = message
            .gas_limit()
            .or_else(|| {
                // Querying gas limit. Fails in cases of `GasTree` invalidations.
                let gas_limit = GasHandlerOf::<T>::get_limit(origin_msg)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                // If available gas is greater then threshold,
                // than threshold can be used.
                (gas_limit >= threshold).then_some(threshold)
            })
            .unwrap_or_default();

        // Converting payload into string.
        //
        // Note: for users, trap replies always contain
        // string explanation of the error.
        let message = match message.exit_code() {
            Some(0) | None => message,
            _ => message
                .with_string_payload::<ExecutionErrorReason>()
                .unwrap_or_else(|e| {
                    log::debug!("Failed to decode error to string");
                    e
                }),
        };

        // Converting message into stored one.
        let message = message.into_stored();

        // Taking data for funds manipulations.
        let from = <T::AccountId as Origin>::from_origin(message.source().into_origin());
        let to = <T::AccountId as Origin>::from_origin(message.destination().into_origin());
        let value = message.value().unique_saturated_into();

        // If gas limit can cover threshold, message will be added to mailbox,
        // task created and funds reserved.
        let expiration = if gas_limit >= threshold {
            // Figuring out hold bound for given gas limit.
            let hold = HoldBound::<T>::by(CostsPerBlockOf::<T>::mailbox()).maximum_for(gas_limit);

            // Validating holding duration.
            if hold.expected_duration().is_zero() {
                unreachable!("Threshold for mailbox invalidated")
            }

            // Cutting gas for storing in mailbox.
            GasHandlerOf::<T>::cut(origin_msg, message.id(), gas_limit)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Reserving value from source for future transfer or unreserve.
            CurrencyOf::<T>::reserve(&from, value)
                .unwrap_or_else(|e| unreachable!("Unable to reserve requested value {:?}", e));

            // Inserting message in mailbox.
            MailboxOf::<T>::insert(message.clone(), hold.expected())
                .unwrap_or_else(|e| unreachable!("Mailbox corrupted! {:?}", e));

            // Adding removal request in task pool.
            TaskPoolOf::<T>::add(
                hold.expected(),
                ScheduledTask::RemoveFromMailbox(to, message.id()),
            )
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

            // Real expiration block.
            Some(hold.expected())
        } else {
            // Permanently transferring funds.
            CurrencyOf::<T>::transfer(&from, &to, value, ExistenceRequirement::AllowDeath)
                .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));

            // No expiration block due to absence of insertion in storage.
            None
        };

        // Depositing appropriate event.
        Self::deposit_event(Event::UserMessageSent {
            message,
            expiration,
        });
    }
}
