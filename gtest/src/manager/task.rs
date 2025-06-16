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

//! Implementation of the `TaskHandler` trait for the `ExtManager`.

use super::ExtManager;
use crate::{state::actors::Actors, Gas};
use core_processor::common::JournalHandler;
use gear_common::{scheduler::StorageType, Gas as GearCommonGas};
use gear_core::{
    gas_metering::TaskWeights,
    ids::{ActorId, MessageId, ReservationId},
    message::{DispatchKind, ReplyMessage},
    tasks::{ScheduledTask, TaskHandler, VaraScheduledTask},
};
use gear_core_errors::{ErrorReplyReason, SignalCode};

pub(crate) fn get_maximum_task_gas(task: &VaraScheduledTask<ActorId>) -> Gas {
    use ScheduledTask::*;
    let weights = TaskWeights::default();
    match task {
        RemoveFromMailbox(_, _) => Gas(weights.remove_from_mailbox.ref_time),
        RemoveFromWaitlist(_, _) => Gas(weights.remove_from_waitlist.ref_time),
        WakeMessage(_, _) => Gas(weights
            .wake_message
            .ref_time
            .max(weights.wake_message_no_wake.ref_time)),
        SendDispatch(_) => Gas(weights.send_dispatch.ref_time),
        SendUserMessage { .. } => Gas(weights
            .send_user_message_to_mailbox
            .ref_time
            .max(weights.send_user_message.ref_time)),
        RemoveGasReservation(_, _) => Gas(weights.remove_gas_reservation.ref_time),
    }
}

impl TaskHandler<ActorId, MessageId, bool> for ExtManager {
    fn remove_from_mailbox(&mut self, user_id: ActorId, message_id: MessageId) -> GearCommonGas {
        let message = ReplyMessage::auto(message_id);

        self.gas_tree
            .create(user_id, message.id(), 0, true)
            .expect("failed to create gas tree node");

        let mailboxed = self
            .read_mailbox_message(user_id, message_id)
            .expect("failed to claim value from mailbox");

        let dispatch = message.into_stored_dispatch(
            mailboxed.destination(),
            mailboxed.source(),
            mailboxed.id(),
        );

        self.dispatches.push_back(dispatch);

        TaskWeights::default().remove_from_mailbox.ref_time
    }

    fn remove_from_waitlist(
        &mut self,
        program_id: ActorId,
        message_id: MessageId,
    ) -> GearCommonGas {
        let waitlisted = self
            .wake_dispatch_impl(program_id, message_id)
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "TaskHandler::remove_from_waitlist: failed waking dispatch. \
                Program id - {program_id}, waking message - {message_id} \
                Got error - {e:?}."
                );

                unreachable!("{err_msg}");
            });

        self.send_signal(
            message_id,
            waitlisted.destination(),
            SignalCode::RemovedFromWaitlist,
        );

        if !waitlisted.is_reply() && waitlisted.kind() != DispatchKind::Signal {
            let err = ErrorReplyReason::RemovedFromWaitlist;

            let err_payload = err
                .to_string()
                .into_bytes()
                .try_into()
                .expect("internal error: error reply reason bytes size is too big");

            let trap_reply = ReplyMessage::system(message_id, err_payload, 0, err);

            if Actors::is_program(waitlisted.source()) {
                let trap_dispatch =
                    trap_reply.into_stored_dispatch(program_id, waitlisted.source(), message_id);

                self.gas_tree
                    .split(
                        trap_dispatch.is_reply(),
                        waitlisted.id(),
                        trap_dispatch.id(),
                    )
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted: {e:?}"));
                self.dispatches.push_back(trap_dispatch);
            } else {
                let trap_message =
                    trap_reply.into_stored(program_id, waitlisted.source(), message_id);
                self.log.push(trap_message);
            }
        }

        self.consume_and_retrieve(waitlisted.id());

        if waitlisted.kind() == DispatchKind::Init {
            let origin = waitlisted.source();
            self.init_failure(program_id, origin);
        }

        TaskWeights::default().remove_from_waitlist.ref_time
    }

    fn wake_message(&mut self, program_id: ActorId, message_id: MessageId) -> GearCommonGas {
        if let Ok(dispatch) = self.wake_dispatch_impl(program_id, message_id) {
            self.dispatches.push_back(dispatch);
            TaskWeights::default().wake_message.ref_time
        } else {
            TaskWeights::default().wake_message_no_wake.ref_time
        }
    }

    fn send_dispatch(&mut self, stashed_message_id: MessageId) -> GearCommonGas {
        let (dispatch, hold_interval) = self
            .dispatches_stash
            .remove(&stashed_message_id)
            .unwrap_or_else(|| unreachable!("TaskPool corrupted"));

        self.charge_for_hold(dispatch.id(), hold_interval, StorageType::DispatchStash);

        self.dispatches.push_back(dispatch.into());
        TaskWeights::default().send_dispatch.ref_time
    }

    fn send_user_message(
        &mut self,
        stashed_message_id: MessageId,
        to_mailbox: bool,
    ) -> GearCommonGas {
        let (message, hold_interval) = self
            .dispatches_stash
            .remove(&stashed_message_id)
            .map(|(dispatch, interval)| (dispatch.into_parts().1, interval))
            .unwrap_or_else(|| unreachable!("TaskPool corrupted!"));

        self.charge_for_hold(message.id(), hold_interval, StorageType::DispatchStash);

        let mailbox_message = message.clone().try_into().unwrap_or_else(|e| {
            unreachable!("invalid message: can't be converted to user message {e:?}")
        });

        self.send_user_message_after_delay(mailbox_message, to_mailbox);
        if to_mailbox {
            TaskWeights::default().send_user_message_to_mailbox.ref_time
        } else {
            TaskWeights::default().send_user_message.ref_time
        }
    }

    fn remove_gas_reservation(
        &mut self,
        program_id: ActorId,
        reservation_id: ReservationId,
    ) -> GearCommonGas {
        let _slot = self.remove_gas_reservation_impl(program_id, reservation_id);
        TaskWeights::default().remove_gas_reservation.ref_time
    }
}
