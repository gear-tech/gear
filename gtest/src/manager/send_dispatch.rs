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

use super::*;
use gear_core::message::UserMessage;

impl ExtManager {
    /// Insert message into the delayed queue.
    pub(crate) fn send_delayed_dispatch(
        &mut self,
        origin_msg: MessageId,
        dispatch: Dispatch,
        delay: u32,
        to_user: bool,
        reservation: Option<ReservationId>,
    ) {
        if delay.is_zero() {
            let err_msg = "send_delayed_dispatch: delayed sending with zero delay appeared";

            unreachable!("{err_msg}");
        }

        let message_id = dispatch.id();

        if self.dispatches_stash.contains_key(&message_id) {
            let err_msg = format!(
                "send_delayed_dispatch: stash already has the message id - {id}",
                id = dispatch.id()
            );

            unreachable!("{err_msg}");
        }

        // Validating dispatch wasn't sent from system with delay.
        if dispatch.is_error_reply() || matches!(dispatch.kind(), DispatchKind::Signal) {
            let err_msg = format!(
                "send_delayed_dispatch: message of an invalid kind is sent: {kind:?}",
                kind = dispatch.kind()
            );

            unreachable!("{err_msg}");
        }

        let mut to_mailbox = false;

        let sender_node = reservation
            .map(Origin::into_origin)
            .unwrap_or_else(|| origin_msg.into_origin());

        let from = dispatch.source();
        let value = dispatch.value();

        let hold_builder = HoldBoundBuilder::new(StorageType::DispatchStash);

        let delay_hold = hold_builder.duration(self, delay);
        let gas_for_delay = delay_hold.lock_amount(self);

        let interval_finish = if to_user {
            let threshold = RentWeights::default().mailbox_threshold.ref_time;

            let gas_limit = dispatch
                .gas_limit()
                .or_else(|| {
                    let gas_limit = self.gas_tree.get_limit(sender_node).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "send_delayed_dispatch: failed getting message gas limit. \
                                Lock sponsor id - {sender_node:?}. Got error - {e:?}"
                        );

                        unreachable!("{err_msg}");
                    });

                    (gas_limit.saturating_sub(gas_for_delay) >= threshold).then_some(threshold)
                })
                .unwrap_or_default();

            to_mailbox = !dispatch.is_reply() && gas_limit >= threshold;

            let gas_amount = if to_mailbox {
                gas_for_delay.saturating_add(gas_limit)
            } else {
                gas_for_delay
            };

            self.gas_tree
                .cut(sender_node, message_id, gas_amount)
                .unwrap_or_else(|e| {
                    let sender_node = sender_node.cast::<PlainNodeId>();
                    let err_msg = format!(
                        "send_delayed_dispatch: failed creating cut node. \
                        Origin node - {sender_node:?}, cut node id - {id}, amount - {gas_amount}. \
                        Got error - {e:?}",
                        id = dispatch.id()
                    );

                    unreachable!("{err_msg}");
                });

            if !to_mailbox {
                self.gas_tree
                    .split_with_value(
                        true,
                        origin_msg,
                        MessageId::generate_reply(dispatch.id()),
                        0,
                    )
                    .expect("failed to split with value gas node");
            }

            if let Some(reservation_id) = reservation {
                self.remove_gas_reservation_with_task(dispatch.source(), reservation_id)
            }

            // Locking funds for holding.
            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                unreachable!("{err_msg}");
            });

            self.gas_tree.lock(dispatch.id(), lock_id, delay_hold.lock_amount(self))
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_delayed_dispatch: failed locking gas for the user message stash hold. \
                        Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                        message_id = dispatch.id(),
                        lock = delay_hold.lock_amount(self));
                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: user message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        } else {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => self
                    .gas_tree
                    .split_with_value(
                        dispatch.is_reply(),
                        sender_node,
                        dispatch.id(),
                        gas_limit.saturating_add(gas_for_delay),
                    )
                    .expect("GasTree corrupted"),

                (None, None) => self
                    .gas_tree
                    .split(dispatch.is_reply(), sender_node, dispatch.id())
                    .expect("GasTree corrupted"),
                (Some(gas_limit), Some(reservation_id)) => {
                    let err_msg = format!(
                        "send_delayed_dispatch: sending dispatch with gas from reservation isn't implemented. \
                        Message - {message_id}, sender - {sender}, gas limit - {gas_limit}, reservation - {reservation_id}",
                        message_id = dispatch.id(),
                        sender = dispatch.source(),
                    );

                    unreachable!("{err_msg}");
                }

                (None, Some(reservation_id)) => {
                    self.gas_tree
                        .split(dispatch.is_reply(), reservation_id, dispatch.id())
                        .expect("GasTree corrupted");
                    self.remove_gas_reservation_with_task(dispatch.source(), reservation_id);
                }
            }

            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                unreachable!("{err_msg}");
            });

            self.gas_tree
                .lock(dispatch.id(), lock_id, delay_hold.lock_amount(self))
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                    "send_delayed_dispatch: failed locking gas for the program message stash hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                    message_id = dispatch.id(),
                    lock = delay_hold.lock_amount(self)
                );

                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: program message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        };

        // It's necessary to deposit value so the source would have enough
        // balance locked (in gear-bank) for future value processing.
        //
        // In case of error replies, we don't need to do it, since original
        // message value is already on locked balance in gear-bank.
        if !dispatch.value().is_zero() && !dispatch.is_error_reply() {
            self.bank.deposit_value(from, value, false);
        }

        let message_id = dispatch.id();

        let start_bn = self.block_height();
        let delay_interval = Interval {
            start: start_bn,
            finish: interval_finish,
        };

        self.dispatches_stash
            .insert(message_id, (dispatch.into_stored_delayed(), delay_interval));

        let task = if to_user {
            ScheduledTask::SendUserMessage {
                message_id,
                to_mailbox,
            }
        } else {
            ScheduledTask::SendDispatch(message_id)
        };

        let task_bn = self.block_height().saturating_add(delay);

        self.task_pool.add(task_bn, task).unwrap_or_else(|e| {
            let err_msg = format!(
                "send_delayed_dispatch: failed adding task for delayed message sending. \
                    Message to user - {to_user}, message id - {message_id}. Got error - {e:?}"
            );

            unreachable!("{err_msg}");
        });
        self.on_task_pool_change();
    }

    pub(crate) fn send_user_message(
        &mut self,
        origin_msg: MessageId,
        message: Message,
        reservation: Option<ReservationId>,
    ) {
        let threshold = RentWeights::default().mailbox_threshold.ref_time;

        let msg_id = reservation
            .map(Origin::into_origin)
            .unwrap_or_else(|| origin_msg.into_origin());

        let gas_limit = message
            .gas_limit()
            .or_else(|| {
                let gas_limit = self.gas_tree.get_limit(msg_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed getting message gas limit. \
                            Lock sponsor id - {msg_id}. Got error - {e:?}"
                    );

                    unreachable!("{err_msg}");
                });

                // If available gas is greater then threshold,
                // than threshold can be used.
                (gas_limit >= threshold).then_some(threshold)
            })
            .unwrap_or_default();

        let from = message.source();
        let to = message.destination();
        let value = message.value();
        let is_error_reply = message.is_error_reply();

        let stored_message = message.into_stored();
        let message: UserMessage = stored_message
            .clone()
            .try_into()
            .expect("failed to convert stored message to user message");

        // It's necessary to deposit value so the source would have enough
        // balance locked (in gear-bank) for future value processing.
        //
        // In case of error replies, we don't need to do it, since original
        // message value is already on locked balance in gear-bank.
        if value != 0 && !is_error_reply {
            self.bank.deposit_value(from, value, false);
        }

        let _ = if message.details().is_none() && gas_limit >= threshold {
            let hold = HoldBoundBuilder::new(StorageType::Mailbox).maximum_for(self, gas_limit);

            if hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_user_message: mailbox message got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::Mailbox)
                );

                unreachable!("{err_msg}");
            }

            self.gas_tree
                .cut(msg_id, message.id(), gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed creating cut node. \
                        Origin node - {msg_id}, cut node id - {id}, amount - {gas_limit}. \
                        Got error - {e:?}",
                        id = message.id()
                    );

                    unreachable!("{err_msg}");
                });

            self.gas_tree
                .lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                        message_id = message.id(),
                    );

                    unreachable!("{err_msg}");
                });

            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .expect("failed to convert user message to user stored message");

            self.mailbox
                .insert(message, hold.expected())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    unreachable!("{err_msg}");
                });

            self.task_pool
                .add(
                    hold.expected(),
                    ScheduledTask::RemoveFromMailbox(to, message_id),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                        bn = hold.expected()
                    );

                    unreachable!("{err_msg}");
                });

            Some(hold.expected())
        } else {
            self.bank.transfer_value(from, to, value);

            if message.details().is_none() {
                // Creating auto reply message.
                let reply_message = ReplyMessage::auto(message.id());

                self.gas_tree
                    .split_with_value(true, origin_msg, reply_message.id(), 0)
                    .expect("GasTree corrupted");
                // Converting reply message into appropriate type for queueing.
                let reply_dispatch = reply_message.into_stored_dispatch(
                    message.destination(),
                    message.source(),
                    message.id(),
                );

                self.dispatches.push_back(reply_dispatch);
            }

            None
        };
        self.events.push(stored_message);

        if let Some(reservation_id) = reservation {
            self.remove_gas_reservation_with_task(message.source(), reservation_id);
        }
    }

    pub(crate) fn send_user_message_after_delay(&mut self, message: UserMessage, to_mailbox: bool) {
        let from = message.source();
        let to = message.destination();
        let value = message.value();

        let _ = if to_mailbox {
            let gas_limit = self.gas_tree.get_limit(message.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed getting message gas limit. \
                        Message id - {message_id}. Got error - {e:?}",
                    message_id = message.id()
                );

                unreachable!("{err_msg}");
            });

            let hold = HoldBoundBuilder::new(StorageType::Mailbox).maximum_for(self, gas_limit);

            if hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_user_message_after_delay: mailbox message (after delay) got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::Mailbox)
                );

                unreachable!("{err_msg}");
            }

            self.gas_tree.lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                        message_id = message.id(),
                    );

                    unreachable!("{err_msg}");
                });

            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .expect("failed to convert user message to user stored message");
            self.mailbox
                .insert(message, hold.expected())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    unreachable!("{err_msg}");
                });

            // Adding removal request in task pool

            self.task_pool
                .add(
                    hold.expected(),
                    ScheduledTask::RemoveFromMailbox(to, message_id),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                    "send_user_message_after_delay: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                    bn = hold.expected()
                );

                    unreachable!("{err_msg}");
                });

            Some(hold.expected())
        } else {
            self.bank.transfer_value(from, to, value);

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
                self.dispatches.push_back(reply_dispatch);
            }

            self.consume_and_retrieve(message.id());
            None
        };

        self.events.push(message.into());
    }
}
