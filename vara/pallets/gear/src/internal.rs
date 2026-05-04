// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
    AccountIdOf, BalanceOf, Config, CostsPerBlockOf, DispatchStashOf, Event, ExtManager,
    GasBalanceOf, GasHandlerOf, GasNodeIdOf, GearBank, MailboxOf, Pallet, QueueOf,
    SchedulingCostOf, TaskPoolOf, WaitlistOf,
};
use alloc::{collections::BTreeSet, format};
use common::{
    GasTree, LockId, LockableTree, Origin,
    event::{
        MessageWaitedReason, MessageWaitedRuntimeReason::*, MessageWokenReason, Reason::*,
        UserMessageReadReason,
    },
    gas_provider::{GasNodeId, Imbalance},
    scheduler::*,
    storage::*,
};
use core::{
    cmp::{Ord, Ordering},
    num::NonZero,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{ActorId, MessageId, ReservationId, prelude::*},
    message::{
        Dispatch, DispatchKind, Message, ReplyMessage, StoredDispatch, UserMessage,
        UserStoredMessage,
    },
    tasks::ScheduledTask,
};
use sp_runtime::traits::{Get, One, SaturatedConversion, Saturating, UniqueSaturatedInto, Zero};

type MailboxError<T> = <<<T as Config>::Messenger as Messenger>::Mailbox as Mailbox>::OutputError;
type WaitlistError<T> =
    <<<T as Config>::Messenger as Messenger>::Waitlist as Waitlist>::OutputError;

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
        let gas_limit = GasHandlerOf::<T>::get_limit(message_id).unwrap_or_else(|e| {
            let err_msg = format!(
                "HoldBoundBuilder::maximum_for_message: failed getting message gas limit. \
                Message id - {message_id}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

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
        Some(self.cmp(other))
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

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InheritorForError {
    Cyclic { holders: BTreeSet<ActorId> },
    NotFound,
}

// Internal functionality implementation.
impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // Reset of all storages.
    #[cfg(feature = "runtime-benchmarks")]
    pub(crate) fn reset() {
        use common::{CodeStorage, GasProvider, ProgramStorage};

        <<T as Config>::ProgramStorage as ProgramStorage>::reset();
        <T as Config>::CodeStorage::reset();
        <T as Config>::GasProvider::reset();
        <T as Config>::Scheduler::reset();
        <T as Config>::Messenger::reset();
    }

    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub(crate) fn spend_burned(id: impl Into<GasNodeIdOf<T>>, amount: GasBalanceOf<T>) {
        Self::spend_gas(None, id, amount)
    }

    pub fn spend_gas(
        to: Option<AccountIdOf<T>>,
        id: impl Into<GasNodeIdOf<T>>,
        amount: GasBalanceOf<T>,
    ) {
        let id = id.into();

        // If amount is zero, nothing to do.
        if amount.is_zero() {
            return;
        }

        // Spending gas amount from `GasNode`.
        GasHandlerOf::<T>::spend(id, amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed spending gas. Message id - {id}, amount - {amount}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        // Querying external id. Fails in cases of `GasTree` invalidations.
        let (external, multiplier, _) = GasHandlerOf::<T>::get_origin_node(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed getting origin node for the current one. Message id - {id}, Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        // Transferring reserved funds from external user to destination.
        if let Some(account_id) = &to {
            GearBank::<T>::spend_gas_to(account_id, &external, amount, multiplier)
        } else {
            GearBank::<T>::spend_gas(&external, amount, multiplier)
        }.unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed spending value for gas in gear bank. \
                Spender - {external:?}, spending to - {to:?}, amount - {amount}, multiplier - {multiplier:?}. \
                Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        })
    }

    /// Consumes message by given `MessageId` or gas reservation by `ReservationId`.
    ///
    /// Updates currency and balances data on imbalance creation.
    pub(crate) fn consume_and_retrieve(id: impl Into<GasNodeIdOf<T>>) {
        let id = id.into();

        // Consuming `GasNode`, returning optional outcome with imbalance.
        let outcome = GasHandlerOf::<T>::consume(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "consume_and_retrieve: failed consuming the rest of gas. Message id - {id:?}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        // Unreserving funds, if imbalance returned.
        if let Some((imbalance, multiplier, external)) = outcome {
            // Peeking numeric value from negative imbalance.
            let gas_left = imbalance.peek();

            // Unreserving funds, if left non-zero amount of gas.
            if !gas_left.is_zero() {
                log::debug!(
                    "Consumed message {id}. Unreserving {gas_left} (gas) from {external:?}"
                );

                GearBank::<T>::withdraw_gas(&external, gas_left, multiplier)
                    .unwrap_or_else(|e| {
                        let err_msg = format!(
                            "consume_and_retrieve: failed withdrawing value for gas from bank. \
                            Message id - {id:?}, withdraw to - {external:?}, amount - {gas_left}, multiplier - {multiplier:?}. \
                            Got error - {e:?}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    });
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
                let prepaid = GasHandlerOf::<T>::unlock_all(id, lock_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "charge_for_hold: failed unlocking locked gas. Message id - {id:?}, lock id - {lock_id:?}, \
                        Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                });
                prepaid.min(duration.saturating_mul(cost))
            },
        );

        // Spending gas, if need.
        if !amount.is_zero() {
            // Spending gas for rent to the rent pool if any.
            // If there is no rent pool id then funds will be spent to the block author.
            Self::spend_gas(<T as Config>::RentPoolId::get(), id, amount)
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

        // Taking data for tasks and error logs.
        let message_id = dispatch.id();
        let destination = dispatch.destination();

        // Validating duration.
        if hold.expected_duration().is_zero() {
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed getting message gas limit. Message id - {message_id}. \
                        Got error - {e:?}",
                    message_id = dispatch.id()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            let err_msg = format!(
                "wait_dispatch: message got zero duration hold bound for waitlist. \
                Requested duration - {duration:?}, gas limit - {gas_limit}, \
                wait reason - {reason:?}, message id - {}.",
                dispatch.id(),
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // Locking funds for holding.
        let lock_id = hold.lock_id().unwrap_or_else(|| {
            // Waitlist storage is guaranteed to have an associated lock id
            let err_msg = "wait_dispatch: No associated lock id for the waitlist storage";

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
        GasHandlerOf::<T>::lock(message_id, lock_id, hold.lock_amount()).unwrap_or_else(|e| {
            let err_msg = format!(
                "wait_dispatch: failed locking gas for the waitlist hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                lock = hold.lock_amount()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        // Querying origin message id. Fails in cases of `GasTree` invalidations.
        let origin_msg = GasHandlerOf::<T>::get_origin_key(GasNodeId::Node(message_id))
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed getting origin node for the current one. \
                    Message id - {message_id}, Got error - {e:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

        match reason {
            Runtime(WaitForCalled | WaitUpToCalledFull) => {
                let expected = hold.expected();
                let task = ScheduledTask::WakeMessage(destination, message_id);

                if !TaskPoolOf::<T>::contains(&expected, &task) {
                    TaskPoolOf::<T>::add(expected, task).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "wait_dispatch: failed adding task for waking message. \
                            Expected bn - {expected:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                    });
                }
            }
            Runtime(WaitCalled | WaitUpToCalled) => {
                TaskPoolOf::<T>::add(
                    hold.expected(),
                    ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "wait_dispatch: failed adding task for removing message from waitlist. \
                        Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            }
            System(reason) => match reason {},
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
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed inserting message to the wailist. \
                    Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                    bn = hold.expected(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            })
    }

    /// Wakes dispatch from waitlist, permanently charged for hold with
    /// appropriate event depositing, if found.
    pub(crate) fn wake_dispatch(
        program_id: ActorId,
        message_id: MessageId,
        reason: MessageWokenReason,
    ) -> Result<StoredDispatch, WaitlistError<T>> {
        // Removing dispatch from waitlist, doing wake requirements if found.
        WaitlistOf::<T>::remove(program_id, message_id)
            .map(|v| Self::wake_dispatch_requirements(v, reason))
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
    ) -> Result<UserStoredMessage, MailboxError<T>> {
        // Removing message from mailbox, doing read requirements if found.
        MailboxOf::<T>::remove(user_id, message_id)
            .map(|v| Self::read_message_requirements(v, reason))
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
        let user_id = mailboxed.destination().cast();
        let from = mailboxed.source().cast();

        // Transferring reserved funds, associated with the message.
        GearBank::<T>::transfer_value(&from, &user_id, mailboxed.value().unique_saturated_into())
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "read_message_requirements: failed transferring value on gear bank. \
                    Sender - {from:?}, destination - {user_id:?}, value - {value}. Got error - {e:?}",
                    value = mailboxed.value(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

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
            let err_msg = "send_delayed_dispatch: delayed sending with zero delay appeared";

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // Validating stash from duplicates.
        if DispatchStashOf::<T>::contains_key(&dispatch.id()) {
            let err_msg = format!(
                "send_delayed_dispatch: stash already has the message id - {id}",
                id = dispatch.id()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // Validating dispatch wasn't sent from system with delay.
        if dispatch.is_error_reply() || matches!(dispatch.kind(), DispatchKind::Signal) {
            let err_msg = format!(
                "send_delayed_dispatch: message of an invalid kind is sent: {kind:?}",
                kind = dispatch.kind()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // Indicates that message goes to mailbox and gas should be charged for holding
        let mut to_mailbox = false;

        // Sender node of the dispatch.
        let sender_node = reservation
            .map(GasNodeId::Reservation)
            .unwrap_or_else(|| origin_msg.into());

        // Taking data for funds manipulations.
        let from = dispatch.source().cast();
        let value = dispatch.value().unique_saturated_into();

        // `HoldBound` builder.
        let hold_builder = HoldBoundBuilder::<T>::new(StorageType::DispatchStash);

        // Calculating correct gas amount for delay.
        let delay_hold = hold_builder.duration(delay.saturated_into());
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
                    let gas_limit = GasHandlerOf::<T>::get_limit(sender_node).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "send_delayed_dispatch: failed getting message gas limit. \
                                Lock sponsor id - {sender_node}. Got error - {e:?}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                    });

                    // If available gas is greater then threshold,
                    // than threshold can be used.
                    //
                    // Here we subtract gas for delay from gas limit to prevent
                    // case when gasless message steal threshold from gas for
                    // delay payment and delay payment becomes insufficient.
                    (gas_limit.saturating_sub(gas_for_delay) >= threshold).then_some(threshold)
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

            GasHandlerOf::<T>::cut(sender_node, dispatch.id(), gas_amount).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_delayed_dispatch: failed creating cut node. \
                        Origin node - {sender_node}, cut node id - {id}, amount - {gas_amount}. \
                        Got error - {e:?}",
                    id = dispatch.id()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

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

            // Locking funds for holding.
            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
            GasHandlerOf::<T>::lock(dispatch.id(), lock_id, delay_hold.lock_amount())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                    "send_delayed_dispatch: failed locking gas for the user message stash hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                    message_id = dispatch.id(),
                    lock = delay_hold.lock_amount()
                );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration().is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: user message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = CostsPerBlockOf::<T>::by_storage_type(StorageType::DispatchStash)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        } else {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => Self::split_with_value(
                    sender_node,
                    dispatch.id(),
                    gas_limit.saturating_add(gas_for_delay),
                    dispatch.is_reply(),
                ),
                (None, None) => Self::split(sender_node, dispatch.id(), dispatch.is_reply()),
                (Some(gas_limit), Some(reservation_id)) => {
                    // TODO: #1828
                    let err_msg = format!(
                        "send_delayed_dispatch: sending dispatch with gas from reservation isn't implemented. \
                        Message - {message_id}, sender - {sender}, gas limit - {gas_limit}, reservation - {reservation_id}",
                        message_id = dispatch.id(),
                        sender = dispatch.source(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                }
                (None, Some(reservation_id)) => {
                    Self::split(reservation_id, dispatch.id(), dispatch.is_reply());
                    Self::remove_gas_reservation_with_task(dispatch.source(), reservation_id);
                }
            }

            // Locking funds for holding.
            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
            GasHandlerOf::<T>::lock(dispatch.id(), lock_id, delay_hold.lock_amount())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_delayed_dispatch: failed locking gas for the program message stash hold. \
                        Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                        message_id = dispatch.id(),
                        lock = delay_hold.lock_amount()
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration().is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: program message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = CostsPerBlockOf::<T>::by_storage_type(StorageType::DispatchStash)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        };

        if dispatch.is_error_reply() {
            let err_msg = "send_delayed_dispatch: delayed sending of error reply appeared";

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // It's necessary to deposit value so the source would have enough
        // balance locked (in gear-bank) for future value processing.
        if !dispatch.value().is_zero() {
            // Reserving value from source for future transfer or unreserve.
            GearBank::<T>::deposit_value(&from, value, false).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_delayed_dispatch: failed depositting value on gear bank. \
                        From - {from:?}, value - {value:?}. Got error - {e:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
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
        DispatchStashOf::<T>::insert(message_id, (dispatch.into_stored_delayed(), delay_interval));

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
        TaskPoolOf::<T>::add(task_bn, task).unwrap_or_else(|e| {
            let err_msg = format!(
                "send_delayed_dispatch: failed adding task for delayed message sending. \
                    Message to user - {to_user}, message id - {message_id}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
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
                let gas_limit = GasHandlerOf::<T>::get_limit(msg_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed getting message gas limit. \
                            Lock sponsor id - {msg_id}. Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

                // If available gas is greater then threshold,
                // than threshold can be used.
                (gas_limit >= threshold).then_some(threshold)
            })
            .unwrap_or_default();

        // Taking data for error log
        let message_id = message.id();
        let from = message.source();
        let to = message.destination();
        let is_error_reply = message.is_error_reply();

        // Converting message into stored one and user one.
        let message = message.into_stored();
        let message: UserMessage = message.try_into().unwrap_or_else(|_| {
            // Signal message sent to user
            let err_msg = format!(
                "send_user_message: failed conversion from stored into user message. \
                    Message id - {message_id}, program id - {from}, destination - {to}",
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        // Taking data for funds manipulations.
        let from = message.source().cast();
        let to = message.destination().cast::<T::AccountId>();
        let value: BalanceOf<T> = message.value().unique_saturated_into();

        // It's necessary to deposit value so the source would have enough
        // balance locked (in gear-bank) for future value processing.
        //
        // In case of error replies, we don't need to do it, since original
        // message value is already on locked balance in gear-bank.
        if !value.is_zero() && !is_error_reply {
            // Reserving value from source for future transfer or unreserve.
            GearBank::<T>::deposit_value(&from, value, false).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message: failed depositting value on gear bank. \
                                    From - {from:?}, value - {value:?}. Got error - {e:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        }

        // If gas limit can cover threshold, message will be added to mailbox,
        // task created and funds reserved.
        let expiration = if message.details().is_none() && gas_limit >= threshold {
            // Figuring out hold bound for given gas limit.
            let hold = HoldBoundBuilder::<T>::new(StorageType::Mailbox).maximum_for(gas_limit);

            // Validating holding duration.
            if hold.expected_duration().is_zero() {
                let err_msg = format!(
                    "send_user_message: mailbox message got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = CostsPerBlockOf::<T>::by_storage_type(StorageType::Mailbox)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            // Cutting gas for storing in mailbox.
            GasHandlerOf::<T>::cut(msg_id, message.id(), gas_limit).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message: failed creating cut node. \
                        Origin node - {msg_id}, cut node id - {id}, amount - {gas_limit}. \
                        Got error - {e:?}",
                    id = message.id()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Lock the entire `gas_limit` since the only purpose of it is payment for storage.
            GasHandlerOf::<T>::lock(message.id(), LockId::Mailbox, gas_limit).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                    message_id = message.id(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Inserting message in mailbox.
            let message_id = message.id();
            let message: UserStoredMessage = message.clone().try_into().unwrap_or_else(|_| {
                // Replies never sent to mailbox
                let err_msg = format!(
                    "send_user_message: failed conversion from user into user stored message. \
                        Message id - {message_id}, program id - {from:?}, destination - {to:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });
            MailboxOf::<T>::insert(message, hold.expected()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                    bn = hold.expected(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Adding removal request in task pool.
            TaskPoolOf::<T>::add(
                hold.expected(),
                ScheduledTask::RemoveFromMailbox(to.clone(), message_id),
            )
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                    bn = hold.expected()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Real expiration block.
            Some(hold.expected())
        } else {
            // Permanently transferring funds.
            // Note that we have no guarantees of the user account to exist. Since no minimum
            // transfer value is enforced, the transfer can fail. Handle it gracefully.
            GearBank::<T>::transfer_value(&from, &to, value).unwrap_or_else(|e| {
                // errors are ruled out by the protocol guarantees.
                let err_msg =
                    format!("send_user_message: failed to transfer value. Got error: {e:?}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

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
                QueueOf::<T>::queue(reply_dispatch).unwrap_or_else(|e| {
                    let err_msg =
                        format!("send_user_message: failed queuing message. Got error - {e:?}");

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
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
        // Taking data for funds manipulations.
        let from = message.source().cast();
        let to = message.destination().cast::<T::AccountId>();
        let value = message.value().unique_saturated_into();

        // If gas limit can cover threshold, message will be added to mailbox,
        // task created and funds reserved.

        let expiration = if to_mailbox {
            // Querying gas limit. Fails in cases of `GasTree` invalidations.
            let gas_limit = GasHandlerOf::<T>::get_limit(message.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed getting message gas limit. \
                        Message id - {message_id}. Got error - {e:?}",
                    message_id = message.id()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Figuring out hold bound for given gas limit.
            let hold = HoldBoundBuilder::<T>::new(StorageType::Mailbox).maximum_for(gas_limit);

            // Validating holding duration.
            if hold.expected_duration().is_zero() {
                let err_msg = format!(
                    "send_user_message_after_delay: mailbox message (after delay) got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = CostsPerBlockOf::<T>::by_storage_type(StorageType::Mailbox)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            // Lock the entire `gas_limit` since the only purpose of it is payment for storage.
            GasHandlerOf::<T>::lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                        message_id = message.id(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

            // Inserting message in mailbox.
            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .unwrap_or_else(|_| {
                    // Replies never sent to mailbox
                    let err_msg = format!(
                        "send_user_message_after_delay: failed conversion from user into user stored message. \
                        Message id - {message_id}, program id - {from:?}, destination - {to:?}",
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                });
            MailboxOf::<T>::insert(message, hold.expected()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                    bn = hold.expected(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Adding removal request in task pool.
            TaskPoolOf::<T>::add(
                hold.expected(),
                ScheduledTask::RemoveFromMailbox(to.clone(), message_id),
            )
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                    bn = hold.expected()
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Real expiration block.
            Some(hold.expected())
        } else {
            // Transferring reserved funds.
            GearBank::<T>::transfer_value(&from, &to, value).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed transferring value on gear bank. \
                        Sender - {from:?}, destination - {to:?}, value - {value:?}. Got error - {e:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

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
                QueueOf::<T>::queue(reply_dispatch).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed queuing message. Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
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
        program_id: ActorId,
        reservation_id: ReservationId,
    ) {
        let slot = ExtManager::<T>::remove_gas_reservation_impl(program_id, reservation_id);

        let _ = TaskPoolOf::<T>::delete(
            BlockNumberFor::<T>::from(slot.finish),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        );
    }

    pub(crate) fn inheritor_for(
        program_id: ActorId,
        max_depth: NonZero<usize>,
    ) -> Result<(ActorId, BTreeSet<ActorId>), InheritorForError> {
        let max_depth = max_depth.get();

        let mut inheritor = program_id;
        let mut holders: BTreeSet<_> = [program_id].into();

        loop {
            let next_inheritor =
                Self::first_inheritor_of(inheritor).ok_or(InheritorForError::NotFound)?;

            inheritor = next_inheritor;

            // don't insert user or active program
            // because it's the final inheritor we already return
            if Self::first_inheritor_of(next_inheritor).is_none() {
                break;
            }

            if holders.len() == max_depth {
                break;
            }

            if !holders.insert(next_inheritor) {
                return Err(InheritorForError::Cyclic { holders });
            }
        }

        Ok((inheritor, holders))
    }

    /// This fn and [`split_with_value`] works the same: they call api of gas
    /// handler to split (optionally with value) for all cases except reply
    /// sent and contains deposit in storage.
    pub(crate) fn split(
        key: impl Into<GasNodeIdOf<T>> + Clone,
        new_key: impl Into<GasNodeIdOf<T>> + Clone,
        is_reply: bool,
    ) {
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(new_key.clone()) {
            GasHandlerOf::<T>::split(key.clone(), new_key.clone()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "splt: failed to split gas node. Original message id - {key}, \
                        new message id - {new_key}, is_reply - {is_reply}. Got error - {e:?}",
                    key = key.into(),
                    new_key = new_key.into(),
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        }
    }

    /// See ['split'].
    pub(crate) fn split_with_value(
        key: impl Into<GasNodeIdOf<T>> + Clone,
        new_key: impl Into<GasNodeIdOf<T>> + Clone,
        amount: GasBalanceOf<T>,
        is_reply: bool,
    ) {
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(new_key.clone()) {
            GasHandlerOf::<T>::split_with_value(key.clone(), new_key.clone(), amount)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "split_with_value: failed to split with value gas node. Original message id - {key}, \
                        new message id - {new_key}. amount - {amount}, is_reply - {is_reply}. \
                        Got error - {e:?}",
                        key = key.into(),
                        new_key = new_key.into(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
        }
    }

    /// See ['split'].
    pub(crate) fn create(
        origin: T::AccountId,
        key: impl Into<GasNodeIdOf<T>> + Clone,
        amount: GasBalanceOf<T>,
        is_reply: bool,
    ) {
        let multiplier = <T as pallet_gear_bank::Config>::GasMultiplier::get();
        if !is_reply || !GasHandlerOf::<T>::exists_and_deposit(key.clone()) {
            GasHandlerOf::<T>::create(origin.clone(), multiplier, key.clone(), amount)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "create: failed to create gas node. Origin - {origin:?}, message id - {key}, \
                        amount - {amount}, is_reply - {is_reply}. Got error - {e:?}",
                        key = key.into(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
        }
    }
}
