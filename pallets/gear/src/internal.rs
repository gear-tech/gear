// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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
    Authorship, BalanceOf, Config, CostsPerBlockOf, CurrencyOf, DispatchStashOf, Event, ExtManager,
    GasBalanceOf, GasHandlerOf, GasNodeIdOf, MailboxOf, Pallet, QueueOf, SchedulingCostOf,
    TaskPoolOf, WaitlistOf,
};
use alloc::collections::BTreeSet;
use common::{
    event::{
        MessageWaitedReason, MessageWaitedRuntimeReason::*,
        MessageWaitedSystemReason::ProgramIsNotInitialized, MessageWokenReason, ProgramChangeKind,
        Reason::*, UserMessageReadReason,
    },
    gas_provider::{GasNodeId, Imbalance},
    scheduler::*,
    storage::*,
    ActiveProgram, GasPrice, GasTree, LockId, LockableTree, Origin,
};
use core::cmp::{Ord, Ordering};
use core_processor::common::ActorExecutionErrorReplyReason;
use frame_support::traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{MessageId, ProgramId, ReservationId},
    message::{
        Dispatch, DispatchKind, Message, ReplyMessage, StoredDispatch, UserMessage,
        UserStoredMessage,
    },
};
use sp_runtime::{
    traits::{
        Bounded, CheckedAdd, Get, One, SaturatedConversion, Saturating, UniqueSaturatedInto, Zero,
    },
    DispatchError,
};

/// [`HoldBound`] builder
#[derive(Clone, Debug)]
pub(crate) struct HoldBoundBuilder<T: Config> {
    storage_type: StorageType,
    cost: SchedulingCostOf<T>,
}

#[allow(unused)]
impl<T: Config> HoldBoundBuilder<T> {
    /// Creates a builder
    pub fn new(storage_type: StorageType) -> Self {
        Self {
            storage_type,
            cost: CostsPerBlockOf::<T>::by_storage_type(storage_type),
        }
    }

    /// Creates bound to specific given block number.
    pub fn at(self, expected: BlockNumberFor<T>) -> HoldBound<T> {
        HoldBound {
            cost: self.cost,
            expected,
            lock_id: self.storage_type.try_into().ok(),
        }
    }

    /// Creates bound to specific given deadline block number.
    pub fn deadline(self, deadline: BlockNumberFor<T>) -> HoldBound<T> {
        let expected = deadline.saturating_sub(CostsPerBlockOf::<T>::reserve_for());

        self.at(expected)
    }

    /// Creates bound for given duration since current block.
    pub fn duration(self, duration: BlockNumberFor<T>) -> HoldBound<T> {
        let expected = Pallet::<T>::block_number().saturating_add(duration);

        self.at(expected)
    }

    /// Creates maximal available bound for given gas limit.
    pub fn maximum_for(self, gas: GasBalanceOf<T>) -> HoldBound<T> {
        let deadline_duration = gas
            .saturating_div(self.cost.max(One::one()))
            .saturated_into::<BlockNumberFor<T>>();

        let deadline = Pallet::<T>::block_number().saturating_add(deadline_duration);

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
        self.at(Pallet::<T>::block_number())
    }
}

/// Hold bound, specifying cost of storage, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HoldBound<T: Config> {
    /// Cost of storing per block.
    cost: SchedulingCostOf<T>,
    /// Expected block number task to be processed.
    expected: BlockNumberFor<T>,
    /// Appropriate lock id, if exists for storage type
    lock_id: Option<LockId>,
}

// `unused` allowed because some fns may be used in future, but clippy
// doesn't allow this due to `pub(crate)` visibility.
#[allow(unused)]
impl<T: Config> HoldBound<T> {
    /// Returns cost of storing per block, related to current hold bound.
    pub fn cost(&self) -> SchedulingCostOf<T> {
        self.cost
    }

    /// Returns expected block number task to be processed.
    pub fn expected(&self) -> BlockNumberFor<T> {
        self.expected
    }

    /// Appropriate lock id for the HoldBound.
    pub fn lock_id(&self) -> Option<LockId> {
        self.lock_id
    }

    /// Returns expected duration before task will be processed, since now.
    pub fn expected_duration(&self) -> BlockNumberFor<T> {
        self.expected.saturating_sub(Pallet::<T>::block_number())
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
        self.deadline().saturating_sub(Pallet::<T>::block_number())
    }

    /// Returns amount of gas should be locked for rent of the hold afterward.
    pub fn lock_amount(&self) -> GasBalanceOf<T> {
        self.deadline_duration()
            .saturated_into::<GasBalanceOf<T>>()
            .saturating_mul(self.cost())
    }
}

impl<T: Config> PartialOrd for HoldBound<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.expected.partial_cmp(&other.expected)
    }
}

impl<T: Config> Ord for HoldBound<T>
where
    BlockNumberFor<T>: PartialOrd,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.expected.cmp(&other.expected)
    }
}

// Internal functionality implementation.
impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // Reset of all storages.
    #[cfg(feature = "runtime-benchmarks")]
    pub(crate) fn reset() {
        use common::{CodeStorage, GasProvider, PausedProgramStorage, ProgramStorage};

        <<T as Config>::ProgramStorage as PausedProgramStorage>::reset();
        <<T as Config>::ProgramStorage as ProgramStorage>::reset();
        <T as Config>::CodeStorage::reset();
        <T as Config>::GasProvider::reset();
        <T as Config>::Scheduler::reset();
        <T as Config>::Messenger::reset();
    }

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
    pub(crate) fn spend_gas(id: impl Into<GasNodeIdOf<T>>, amount: GasBalanceOf<T>) {
        let id = id.into();

        // If amount is zero, nothing to do.
        if amount.is_zero() {
            return;
        }

        // Spending gas amount from `GasNode`.
        GasHandlerOf::<T>::spend(id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying external id. Fails in cases of `GasTree` invalidations.
        let external = GasHandlerOf::<T>::get_external(id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying actual block author to reward.
        let block_author = Authorship::<T>::author()
            .unwrap_or_else(|| unreachable!("Failed to find block author!"));

        // Converting gas amount into value.
        let value = T::GasPrice::gas_price(amount);

        // Transferring reserved funds from external user to block author.
        Self::transfer_reserved(&external, &block_author, value);
    }

    /// Consumes message by given `MessageId` or gas reservation by `ReservationId`.
    ///
    /// Updates currency and balances data on imbalance creation.
    pub(crate) fn consume_and_retrieve(id: impl Into<GasNodeIdOf<T>>) {
        let id = id.into();

        // Consuming `GasNode`, returning optional outcome with imbalance.
        let outcome = GasHandlerOf::<T>::consume(id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Unreserving funds, if imbalance returned.
        if let Some((imbalance, external)) = outcome {
            // Peeking numeric value from negative imbalance.
            let gas_left = imbalance.peek();

            // Unreserving funds, if left non-zero amount of gas.
            if !gas_left.is_zero() {
                log::debug!(
                    "Consumed message {id}. Unreserving {gas_left} (gas) from {external:?}"
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

    /// Charges for holding in some storage. In order to be placed into a holding storage
    /// a message must lock some funds as a deposit to "book" the storage until some time
    /// in the future.
    /// Basic invariant is that we can't charge for storing an item more than had been deposited,
    /// regardless whether storage costs or the safety margin have changed in the meantime
    /// (via storage migration). The actual "prepaid" amount is determined through releasing
    /// the lock corresponding to the `storage_type` inside the function.
    ///
    /// `id` - parameter convertible to the respective gas node id;
    /// `hold_interval` - determines the time interval to charge rent for;
    /// `storage_type` - storage type that determines the lock and the cost for holding a message.
    pub(crate) fn charge_for_hold(
        id: impl Into<GasNodeIdOf<T>>,
        hold_interval: Interval<BlockNumberFor<T>>,
        storage_type: StorageType,
    ) {
        let id = id.into();

        // Current block number.
        let current = Self::block_number();

        // Deadline of the task.
        //
        // NOTE: the `ReserveFor` value may have changed due to storage migration thereby
        // leading to a mismatch between the amount due and the amount deposited upfront.
        let deadline = hold_interval
            .finish
            .saturating_add(CostsPerBlockOf::<T>::reserve_for());

        // The block number, which was the last paid for hold.
        //
        // Outdated tasks can end up being store for free - this case has to be controlled by
        // a correct selection of the `ReserveFor` constant.
        let paid_until = current.min(deadline);

        // Holding duration.
        let duration: u64 = paid_until
            .saturating_sub(hold_interval.start)
            .unique_saturated_into();

        // Cost per block based on the storage used for holding
        let cost = CostsPerBlockOf::<T>::by_storage_type(storage_type);

        // Amount of gas to be charged for holding.
        // Note: unlocking of all funds that had been locked under the respective lock defines
        // the maximum amount that can be charged for using storage.
        let amount = storage_type.try_into().map_or_else(
            |_| duration.saturating_mul(cost),
            |lock_id| {
                let prepaid = GasHandlerOf::<T>::unlock_all(id, lock_id)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
                prepaid.min(duration.saturating_mul(cost))
            },
        );

        // Spending gas, if need.
        if !amount.is_zero() {
            // Spending gas.
            Self::spend_gas(id, amount)
        }
    }

    /// Adds dispatch into waitlist, deposits event and adds task for waking it.
    pub(crate) fn wait_dispatch(
        dispatch: StoredDispatch,
        duration: Option<BlockNumberFor<T>>,
        reason: MessageWaitedReason,
    ) {
        // `HoldBound` builder.
        let hold_builder = HoldBoundBuilder::<T>::new(StorageType::Waitlist);

        // Maximal hold bound for the message.
        let maximal_hold = hold_builder.clone().maximum_for_message(dispatch.id());

        // Figuring out correct hold bound.
        let hold = if let Some(duration) = duration {
            hold_builder.duration(duration).min(maximal_hold)
        } else {
            maximal_hold
        };

        // Validating duration.
        if hold.expected_duration().is_zero() {
            // TODO: Replace with unreachable call after:
            // - `HoldBound` safety usage stabilized;
            log::error!("Failed to figure out correct wait hold bound");
            return;
        }

        // Locking funds for holding.
        let lock_id = hold.lock_id().unwrap_or_else(|| {
            unreachable!("Waitlist storage is guaranteed to have an associated lock id")
        });
        GasHandlerOf::<T>::lock(dispatch.id(), lock_id, hold.lock_amount())
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying origin message id. Fails in cases of `GasTree` invalidations.
        let origin_msg = GasHandlerOf::<T>::get_origin_key(GasNodeId::Node(dispatch.id()))
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        match reason {
            Runtime(WaitForCalled | WaitUpToCalledFull) => {
                let expected = hold.expected();
                let task = ScheduledTask::WakeMessage(dispatch.destination(), dispatch.id());

                if !TaskPoolOf::<T>::contains(&expected, &task) {
                    TaskPoolOf::<T>::add(expected, task)
                        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
                }
            }
            Runtime(WaitCalled | WaitUpToCalled) | System(ProgramIsNotInitialized) => {
                TaskPoolOf::<T>::add(
                    hold.expected(),
                    ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
                )
                .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
            }
        }

        // Depositing appropriate event.
        Self::deposit_event(Event::MessageWaited {
            id: dispatch.id(),
            origin: origin_msg.ne(&dispatch.id().into()).then_some(origin_msg),
            expiration: hold.expected(),
            reason,
        });

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
        // Expected block number to finish task.
        let expected = hold_interval.finish;

        // Charging for holding.
        Self::charge_for_hold(waitlisted.id(), hold_interval, StorageType::Waitlist);

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::MessageWoken {
            id: waitlisted.id(),
            reason,
        });

        // Delete task, if exists.
        let _ = TaskPoolOf::<T>::delete(
            expected,
            ScheduledTask::RemoveFromWaitlist(waitlisted.destination(), waitlisted.id()),
        );

        waitlisted
    }

    /// Removes message from mailbox, permanently charged for hold with
    /// appropriate event depositing, if found.
    ///
    /// Note: message consumes automatically.
    pub(crate) fn read_message(
        user_id: T::AccountId,
        message_id: MessageId,
        reason: UserMessageReadReason,
    ) -> Option<UserStoredMessage> {
        // Removing message from mailbox, doing read requirements if found.
        MailboxOf::<T>::remove(user_id, message_id)
            .map(|v| Self::read_message_requirements(v, reason))
            .ok()
    }

    /// Charges and deposits event for already taken from mailbox message.
    ///
    /// Note: message auto-consumes, if reason is claim or reply.
    pub(crate) fn read_message_requirements(
        (mailboxed, hold_interval): (UserStoredMessage, Interval<BlockNumberFor<T>>),
        reason: UserMessageReadReason,
    ) -> UserStoredMessage {
        // Expected block number to finish task.
        let expected = hold_interval.finish;

        // Charging for holding.
        Self::charge_for_hold(mailboxed.id(), hold_interval, StorageType::Mailbox);

        // Consuming message.
        Self::consume_and_retrieve(mailboxed.id());

        // Taking data for funds transfer.
        let user_id = <T::AccountId as Origin>::from_origin(mailboxed.destination().into_origin());
        let from = <T::AccountId as Origin>::from_origin(mailboxed.source().into_origin());

        // Transferring reserved funds, associated with the message.
        Self::transfer_reserved(&from, &user_id, mailboxed.value().unique_saturated_into());

        // Depositing appropriate event.
        Pallet::<T>::deposit_event(Event::UserMessageRead {
            id: mailboxed.id(),
            reason,
        });

        // Delete task, if exists.
        let _ = TaskPoolOf::<T>::delete(
            expected,
            ScheduledTask::RemoveFromMailbox(user_id, mailboxed.id()),
        );

        mailboxed
    }

    /// Delays dispatch sending.
    ///
    /// This function adds message into `TaskPool`.
    ///
    /// Function creates gas node for message.
    ///
    /// On processing task at defined block, we check destination, in case of
    /// user and absence of gas node, we don't append message into any storage,
    /// propagating `UserMessageSent` event only.
    pub(crate) fn send_delayed_dispatch(
        origin_msg: MessageId,
        dispatch: Dispatch,
        delay: u32,
        to_user: bool,
        reservation: Option<ReservationId>,
    ) {
        // Validating delay.
        if delay.is_zero() {
            unreachable!("Delayed sending with zero delay appeared");
        }

        // Validating stash from duplicates.
        if DispatchStashOf::<T>::contains_key(&dispatch.id()) {
            unreachable!("Stash logic invalidated!")
        }

        // Validating dispatch wasn't sent from system with delay.
        if dispatch.is_error_reply() || matches!(dispatch.kind(), DispatchKind::Signal) {
            unreachable!("Scheduling logic invalidated");
        }

        // Indicates that message goes to mailbox and gas should be charged for holding
        let mut to_mailbox = false;

        // Sender node of the dispatch.
        let sender_node = reservation
            .map(GasNodeId::Reservation)
            .unwrap_or_else(|| origin_msg.into());

        // Taking data for funds manipulations.
        let from = <T::AccountId as Origin>::from_origin(dispatch.source().into_origin());
        let value = dispatch.value().unique_saturated_into();

        // `HoldBound` builder.
        let hold_builder = HoldBoundBuilder::<T>::new(StorageType::DispatchStash);

        // Calculating correct gas amount for delay.
        let bn_delay = delay.saturated_into::<BlockNumberFor<T>>();
        let delay_hold = hold_builder.clone().duration(bn_delay);
        let gas_for_delay = delay_hold.lock_amount();

        let interval_finish = if to_user {
            // Querying `MailboxThreshold`, that represents minimal amount of gas
            // for message to be added to mailbox.
            let threshold = T::MailboxThreshold::get();

            // Figuring out gas limit for insertion.
            //
            // In case of sending with gas, we use applied gas limit, otherwise
            // finding available funds and trying to take threshold from them.
            let gas_limit = dispatch
                .gas_limit()
                .or_else(|| {
                    // Querying gas limit. Fails in cases of `GasTree` invalidations.
                    let gas_limit = GasHandlerOf::<T>::get_limit(sender_node)
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    // If available gas is greater then threshold,
                    // than threshold can be used.
                    (gas_limit >= threshold).then_some(threshold)
                })
                .unwrap_or_default();

            // Message is going to be inserted into mailbox.
            //
            // No hold bound checks required, because gas_limit isn't less than threshold.
            to_mailbox = !dispatch.is_reply() && gas_limit >= threshold;
            let gas_amount = if to_mailbox {
                // Cutting gas for storing in mailbox.
                gas_for_delay.saturating_add(gas_limit)
            } else {
                gas_for_delay
            };

            GasHandlerOf::<T>::cut(sender_node, dispatch.id(), gas_amount)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Generating gas node for future auto reply.
            // TODO: use `sender_node` (e.g. reservation case) as first argument after #1828.
            if !to_mailbox {
                Self::split_with_value(
                    origin_msg,
                    MessageId::generate_reply(dispatch.id()),
                    0,
                    true,
                );
            }

            // TODO: adapt this line if gasful sending appears for reservations (#1828)
            if let Some(reservation_id) = reservation {
                Self::remove_gas_reservation_with_task(dispatch.source(), reservation_id);
            }

            // Calculating correct hold bound to lock gas.
            let maximal_hold = hold_builder.maximum_for_message(dispatch.id());
            let hold = delay_hold.min(maximal_hold);

            // Locking funds for holding.
            let lock_id = hold.lock_id().unwrap_or_else(|| {
                unreachable!("DispatchStash storage is guaranteed to have an associated lock id")
            });
            GasHandlerOf::<T>::lock(dispatch.id(), lock_id, hold.lock_amount())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            if hold.expected_duration().is_zero() {
                unreachable!("Hold duration cannot be zero");
            }

            hold.expected()
        } else {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => Self::split_with_value(
                    sender_node,
                    dispatch.id(),
                    gas_limit.saturating_add(gas_for_delay),
                    dispatch.is_reply(),
                ),
                (None, None) => Self::split(sender_node, dispatch.id(), dispatch.is_reply()),
                (Some(_gas_limit), Some(_reservation_id)) => {
                    // TODO: #1828
                    unreachable!(
                        "Sending dispatch with gas limit from reservation \
                        is currently unimplemented and there is no way to send such dispatch"
                    );
                }
                (None, Some(reservation_id)) => {
                    Self::split(reservation_id, dispatch.id(), dispatch.is_reply());
                    Self::remove_gas_reservation_with_task(dispatch.source(), reservation_id);
                }
            }

            // `HoldBound` builder.
            let hold_builder = HoldBoundBuilder::<T>::new(StorageType::DispatchStash);

            // Calculating correct hold bound to lock gas.
            let maximal_hold = hold_builder.maximum_for_message(dispatch.id());
            let hold = delay_hold.min(maximal_hold);

            // Locking funds for holding.
            let lock_id = hold.lock_id().unwrap_or_else(|| {
                unreachable!("DispatchStash storage is guaranteed to have an associated lock id")
            });
            GasHandlerOf::<T>::lock(dispatch.id(), lock_id, hold.lock_amount())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            if hold.expected_duration().is_zero() {
                unreachable!("Hold duration cannot be zero");
            }

            hold.expected()
        };

        if !dispatch.value().is_zero() {
            // Reserving value from source for future transfer or unreserve.
            CurrencyOf::<T>::reserve(&from, value)
                .unwrap_or_else(|e| unreachable!("Unable to reserve requested value {:?}", e));
        }

        // Saving id to allow moving dispatch further.
        let message_id = dispatch.id();

        // Add block number of insertion.
        let start_bn = Self::block_number();
        let delay_interval = Interval {
            start: start_bn,
            finish: interval_finish,
        };

        // Adding message into the stash.
        DispatchStashOf::<T>::insert(message_id, (dispatch.into_stored(), delay_interval));

        let task = if to_user {
            ScheduledTask::SendUserMessage {
                message_id,
                to_mailbox,
            }
        } else {
            ScheduledTask::SendDispatch(message_id)
        };

        // Adding removal request in task pool.
        let task_bn = Self::block_number().saturating_add(delay.unique_saturated_into());
        TaskPoolOf::<T>::add(task_bn, task)
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
    }

    /// Sends message to user.
    ///
    /// It may be added to mailbox, if apply requirements.
    pub(crate) fn send_user_message(
        origin_msg: MessageId,
        message: Message,
        reservation: Option<ReservationId>,
    ) {
        // Querying `MailboxThreshold`, that represents minimal amount of gas
        // for message to be added to mailbox.
        let threshold = T::MailboxThreshold::get();

        let msg_id = reservation
            .map(GasNodeId::Reservation)
            .unwrap_or_else(|| origin_msg.into());

        // Figuring out gas limit for insertion.
        //
        // In case of sending with gas, we use applied gas limit, otherwise
        // finding available funds and trying to take threshold from them.
        let gas_limit = message
            .gas_limit()
            .or_else(|| {
                // Querying gas limit. Fails in cases of `GasTree` invalidations.
                let gas_limit = GasHandlerOf::<T>::get_limit(msg_id)
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
        let message = if message.is_error_reply() {
            message
                .with_string_payload::<ActorExecutionErrorReplyReason>()
                .unwrap_or_else(|e| {
                    log::debug!("Failed to decode error to string");
                    e
                })
        } else {
            message
        };

        // Converting message into stored one and user one.
        let message = message.into_stored();
        let message: UserMessage = message
            .try_into()
            .unwrap_or_else(|_| unreachable!("Signal message sent to user"));

        // Taking data for funds manipulations.
        let from = <T::AccountId as Origin>::from_origin(message.source().into_origin());
        let to = <T::AccountId as Origin>::from_origin(message.destination().into_origin());
        let value = message.value().unique_saturated_into();

        // If gas limit can cover threshold, message will be added to mailbox,
        // task created and funds reserved.
        let expiration = if message.details().is_none() && gas_limit >= threshold {
            // Figuring out hold bound for given gas limit.
            let hold = HoldBoundBuilder::<T>::new(StorageType::Mailbox).maximum_for(gas_limit);

            // Validating holding duration.
            if hold.expected_duration().is_zero() {
                unreachable!("Threshold for mailbox invalidated")
            }

            // Cutting gas for storing in mailbox.
            GasHandlerOf::<T>::cut(msg_id, message.id(), gas_limit)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Reserving value from source for future transfer or unreserve.
            CurrencyOf::<T>::reserve(&from, value)
                .unwrap_or_else(|e| unreachable!("Unable to reserve requested value {:?}", e));

            // Lock the entire `gas_limit` since the only purpose of it is payment for storage.
            GasHandlerOf::<T>::lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Inserting message in mailbox.
            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Replies are never added into mailbox"));
            MailboxOf::<T>::insert(message, hold.expected())
                .unwrap_or_else(|e| unreachable!("Mailbox corrupted! {:?}", e));

            // Adding removal request in task pool.
            TaskPoolOf::<T>::add(
                hold.expected(),
                ScheduledTask::RemoveFromMailbox(to, message_id),
            )
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

            // Real expiration block.
            Some(hold.expected())
        } else {
            // Permanently transferring funds.
            CurrencyOf::<T>::transfer(&from, &to, value, ExistenceRequirement::AllowDeath)
                .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));

            if message.details().is_none() {
                // Creating auto reply message.
                let reply_message = ReplyMessage::auto(message.id());

                // Creating `GasNode` for the auto reply.
                // TODO: use `msg_id` (e.g. reservation case) as first argument after #1828.
                Self::split_with_value(origin_msg, reply_message.id(), 0, true);

                // Converting reply message into appropriate type for queueing.
                let reply_dispatch = reply_message.into_stored_dispatch(
                    message.destination(),
                    message.source(),
                    message.id(),
                );

                // Queueing dispatch.
                QueueOf::<T>::queue(reply_dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
            }

            // No expiration block due to absence of insertion in storage.
            None
        };

        // TODO: adapt if gasful sending appears for reservations (#1828)
        if let Some(reservation_id) = reservation {
            Self::remove_gas_reservation_with_task(message.source(), reservation_id);
        }

        // Depositing appropriate event.
        Self::deposit_event(Event::UserMessageSent {
            message,
            expiration,
        });
    }

    /// Sends user message, once delay reached.
    pub(crate) fn send_user_message_after_delay(message: UserMessage, to_mailbox: bool) {
        // Converting payload into string.
        //
        // Note: for users, trap replies always contain
        // string explanation of the error.
        //
        // We don't plan to send delayed error replies yet,
        // but this logic appears here for future purposes.
        let message = if message
            .details()
            .map(|r_d| r_d.to_reply_code().is_error())
            .unwrap_or(false)
        {
            message
        } else {
            message
                .with_string_payload::<ActorExecutionErrorReplyReason>()
                .unwrap_or_else(|e| {
                    log::debug!("Failed to decode error to string");
                    e
                })
        };

        // Taking data for funds manipulations.
        let from = <T::AccountId as Origin>::from_origin(message.source().into_origin());
        let to = <T::AccountId as Origin>::from_origin(message.destination().into_origin());
        let value = message.value().unique_saturated_into();

        // If gas limit can cover threshold, message will be added to mailbox,
        // task created and funds reserved.

        let expiration = if to_mailbox {
            // Querying gas limit. Fails in cases of `GasTree` invalidations.
            let gas_limit = GasHandlerOf::<T>::get_limit(message.id())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Figuring out hold bound for given gas limit.
            let hold = HoldBoundBuilder::<T>::new(StorageType::Mailbox).maximum_for(gas_limit);

            // Validating holding duration.
            if hold.expected_duration().is_zero() {
                unreachable!("Threshold for mailbox invalidated")
            }

            // Lock the entire `gas_limit` since the only purpose of it is payment for storage.
            GasHandlerOf::<T>::lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Inserting message in mailbox.
            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Replies are never added into mailbox"));
            MailboxOf::<T>::insert(message, hold.expected())
                .unwrap_or_else(|e| unreachable!("Mailbox corrupted! {:?}", e));

            // Adding removal request in task pool.
            TaskPoolOf::<T>::add(
                hold.expected(),
                ScheduledTask::RemoveFromMailbox(to, message_id),
            )
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

            // Real expiration block.
            Some(hold.expected())
        } else {
            // Transferring reserved funds.
            Self::transfer_reserved(&from, &to, value);

            // Message is never reply here, because delayed reply sending forbidden.
            if message.details().is_none() {
                // Creating reply message.
                let reply_message = ReplyMessage::auto(message.id());

                // `GasNode` was created on send already.

                // Converting reply message into appropriate type for queueing.
                let reply_dispatch = reply_message.into_stored_dispatch(
                    message.destination(),
                    message.source(),
                    message.id(),
                );

                // Queueing dispatch.
                QueueOf::<T>::queue(reply_dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
            }

            Self::consume_and_retrieve(message.id());
            // No expiration block due to absence of insertion in storage.
            None
        };

        // Depositing appropriate event.
        Self::deposit_event(Event::UserMessageSent {
            message,
            expiration,
        });
    }

    pub(crate) fn remove_gas_reservation_with_task(
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) {
        let slot = ExtManager::<T>::remove_gas_reservation_impl(program_id, reservation_id);

        let _ = TaskPoolOf::<T>::delete(
            BlockNumberFor::<T>::from(slot.finish),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        );
    }

    pub(crate) fn inheritor_for(program_id: ProgramId) -> ProgramId {
        let mut inheritor = program_id;

        let mut visited_ids: BTreeSet<_> = [program_id].into();

        while let Some(id) =
            Self::exit_inheritor_of(inheritor).or_else(|| Self::termination_inheritor_of(inheritor))
        {
            if !visited_ids.insert(id) {
                break;
            }

            inheritor = id
        }

        inheritor
    }

    pub(crate) fn pay_program_rent_impl(
        program_id: ProgramId,
        program: &mut ActiveProgram<BlockNumberFor<T>>,
        from: &T::AccountId,
        block_count: BlockNumberFor<T>,
    ) -> Result<(), DispatchError> {
        let old_expiration_block = program.expiration_block;
        let (new_expiration_block, blocks_to_pay) = old_expiration_block
            .checked_add(&block_count)
            .map(|count| (count, block_count))
            .unwrap_or_else(|| {
                let max = BlockNumberFor::<T>::max_value();

                (max, max - old_expiration_block)
            });

        if blocks_to_pay.is_zero() {
            return Ok(());
        }

        let block_author = Authorship::<T>::author()
            .unwrap_or_else(|| unreachable!("Failed to find block author!"));
        CurrencyOf::<T>::transfer(
            from,
            &block_author,
            Self::rent_fee_for(blocks_to_pay),
            ExistenceRequirement::AllowDeath,
        )?;

        let task = ScheduledTask::PauseProgram(program_id);
        TaskPoolOf::<T>::delete(old_expiration_block, task.clone())
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

        program.expiration_block = new_expiration_block;

        TaskPoolOf::<T>::add(new_expiration_block, task)
            .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

        Self::deposit_event(Event::ProgramChanged {
            id: program_id,
            change: ProgramChangeKind::ExpirationChanged {
                expiration: new_expiration_block,
            },
        });

        Ok(())
    }

    /// This fn and [`split_with_value`] works the same: they call api of gas
    /// handler to split (optionally with value) for all cases except reply
    /// sent and contains deposit in storage.
    pub(crate) fn split(
        key: impl Into<GasNodeIdOf<T>>,
        new_key: impl Into<GasNodeIdOf<T>> + Clone,
        is_reply: bool,
    ) {
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(new_key.clone()) {
            GasHandlerOf::<T>::split(key, new_key)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        }
    }

    /// See ['split'].
    pub(crate) fn split_with_value(
        key: impl Into<GasNodeIdOf<T>>,
        new_key: impl Into<GasNodeIdOf<T>> + Clone,
        amount: GasBalanceOf<T>,
        is_reply: bool,
    ) {
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(new_key.clone()) {
            GasHandlerOf::<T>::split_with_value(key, new_key, amount)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        }
    }

    /// See ['split'].
    pub(crate) fn create(
        origin: T::AccountId,
        key: impl Into<GasNodeIdOf<T>> + Clone,
        amount: GasBalanceOf<T>,
        is_reply: bool,
    ) {
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(key.clone()) {
            GasHandlerOf::<T>::create(origin, key, amount)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        }
    }
}
