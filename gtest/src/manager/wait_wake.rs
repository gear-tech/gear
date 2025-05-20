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

impl ExtManager {
    pub(crate) fn wait_dispatch_impl(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<BlockNumber>,
        reason: MessageWaitedReason,
    ) {
        use MessageWaitedRuntimeReason::*;

        let hold_builder = HoldBoundBuilder::new(StorageType::Waitlist);

        let maximal_hold = hold_builder.maximum_for_message(self, dispatch.id());

        let hold = if let Some(duration) = duration {
            hold_builder.duration(self, duration).min(maximal_hold)
        } else {
            maximal_hold
        };

        let message_id = dispatch.id();
        let destination = dispatch.destination();

        if hold.expected_duration(self).is_zero() {
            let gas_limit = self.gas_tree.get_limit(dispatch.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed getting message gas limit. Message id - {message_id}. \
                        Got error - {e:?}",
                    message_id = dispatch.id()
                );

                unreachable!("{err_msg}");
            });

            let err_msg = format!(
                "wait_dispatch: message got zero duration hold bound for waitlist. \
                Requested duration - {duration:?}, gas limit - {gas_limit}, \
                wait reason - {reason:?}, message id - {}.",
                dispatch.id(),
            );

            unreachable!("{err_msg}");
        }

        // Locking funds for holding.
        let lock_id = hold.lock_id().unwrap_or_else(|| {
            // Waitlist storage is guaranteed to have an associated lock id
            let err_msg = "wait_dispatch: No associated lock id for the waitlist storage";

            unreachable!("{err_msg}");
        });
        self.gas_tree
            .lock(message_id, lock_id, hold.lock_amount(self))
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed locking gas for the waitlist hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                    lock = hold.lock_amount(self)
                );

                unreachable!("{err_msg}");
            });

        match reason {
            MessageWaitedReason::Runtime(WaitForCalled | WaitUpToCalledFull) => {
                let expected = hold.expected();
                let task = ScheduledTask::WakeMessage(destination, message_id);

                if !self.task_pool.contains(&expected, &task) {
                    self.task_pool.add(expected, task).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "wait_dispatch: failed adding task for waking message. \
                            Expected bn - {expected:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        );

                        unreachable!("{err_msg}");
                    });
                    self.on_task_pool_change();
                }
            }
            MessageWaitedReason::Runtime(WaitCalled | WaitUpToCalled) => {
                self.task_pool.add(
                    hold.expected(),
                    ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "wait_dispatch: failed adding task for removing message from waitlist. \
                        Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    unreachable!("{err_msg}");
                });
                self.on_task_pool_change();
            }
            MessageWaitedReason::System(reason) => match reason {},
        }

        self.waitlist.insert(dispatch, hold.expected())
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed inserting message to the wailist. \
                    Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                    bn = hold.expected(),
                );

                unreachable!("{err_msg}");
            });
    }

    pub(crate) fn wake_dispatch_impl(
        &mut self,
        program_id: ActorId,
        message_id: MessageId,
    ) -> Result<StoredDispatch, WaitlistErrorImpl> {
        self.waitlist
            .remove(program_id, message_id)
            .map(|waitlisted_message| self.wake_dispatch_requirements(waitlisted_message))
    }

    pub(crate) fn wake_dispatch_requirements(
        &mut self,
        (waitlisted, hold_interval): (StoredDispatch, Interval<BlockNumber>),
    ) -> StoredDispatch {
        let expected = hold_interval.finish;

        self.charge_for_hold(waitlisted.id(), hold_interval, StorageType::Waitlist);

        let _ = self
            .task_pool
            .delete(
                expected,
                ScheduledTask::RemoveFromWaitlist(waitlisted.destination(), waitlisted.id()),
            )
            .map(|_| {
                self.on_task_pool_change();
            });

        waitlisted
    }
}
