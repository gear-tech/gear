// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::{
    manager::ExtManager, weights::WeightInfo, Config, DispatchStashOf, Event, Pallet, QueueOf,
};
use alloc::string::ToString;
use common::{
    event::{
        MessageWokenRuntimeReason, MessageWokenSystemReason, RuntimeReason, SystemReason,
        UserMessageReadSystemReason,
    },
    paused_program_storage::SessionId,
    scheduler::*,
    storage::*,
    Gas, Origin,
};
use core::cmp;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::{DispatchKind, ReplyMessage},
};
use gear_core_errors::{ErrorReplyReason, SignalCode};

pub fn get_maximum_task_gas<T: Config>(task: &ScheduledTask<T::AccountId>) -> Gas {
    use ScheduledTask::*;

    match task {
        PauseProgram(_) => 0,
        RemoveCode(_) => todo!("#646"),
        RemoveFromMailbox(_, _) => {
            <T as Config>::WeightInfo::tasks_remove_from_mailbox().ref_time()
        }
        RemoveFromWaitlist(_, _) => {
            <T as Config>::WeightInfo::tasks_remove_from_waitlist().ref_time()
        }
        RemovePausedProgram(_) => todo!("#646"),
        WakeMessage(_, _) => cmp::max(
            <T as Config>::WeightInfo::tasks_wake_message().ref_time(),
            <T as Config>::WeightInfo::tasks_wake_message_no_wake().ref_time(),
        ),
        SendDispatch(_) => <T as Config>::WeightInfo::tasks_send_dispatch().ref_time(),
        SendUserMessage { .. } => cmp::max(
            <T as Config>::WeightInfo::tasks_send_user_message_to_mailbox().ref_time(),
            <T as Config>::WeightInfo::tasks_send_user_message().ref_time(),
        ),
        RemoveGasReservation(_, _) => {
            <T as Config>::WeightInfo::tasks_remove_gas_reservation().ref_time()
        }
        RemoveResumeSession(_) => 0,
    }
}

impl<T: Config> TaskHandler<T::AccountId> for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn pause_program(&mut self, _program_id: ProgramId) -> Gas {
        log::debug!("Program rent logic is disabled.");

        0
    }

    fn remove_code(&mut self, _code_id: CodeId) -> Gas {
        todo!("#646")
    }

    fn remove_from_mailbox(&mut self, user_id: T::AccountId, message_id: MessageId) -> Gas {
        // Read reason.
        let reason = UserMessageReadSystemReason::OutOfRent.into_reason();

        let message = ReplyMessage::auto(message_id);

        Pallet::<T>::create(user_id.clone(), message.id(), 0, true);

        // Reading message from mailbox.
        let mailboxed = Pallet::<T>::read_message(user_id, message_id, reason)
            .unwrap_or_else(|| unreachable!("Scheduling logic invalidated!"));

        // Converting reply message into appropriate type for queueing.
        let dispatch = message.into_stored_dispatch(
            mailboxed.destination(),
            mailboxed.source(),
            mailboxed.id(),
        );

        // Queueing dispatch.
        QueueOf::<T>::queue(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

        let gas = <T as Config>::WeightInfo::tasks_remove_from_mailbox().ref_time();
        log::trace!("Task gas: tasks_remove_from_mailbox = {gas}");

        gas
    }

    fn remove_from_waitlist(&mut self, program_id: ProgramId, message_id: MessageId) -> Gas {
        // Wake reason.
        let reason = MessageWokenSystemReason::OutOfRent.into_reason();

        // Waking dispatch.
        let waitlisted = Pallet::<T>::wake_dispatch(program_id, message_id, reason)
            .unwrap_or_else(|| unreachable!("Scheduling logic invalidated!"));

        self.send_signal(
            message_id,
            waitlisted.destination(),
            SignalCode::RemovedFromWaitlist,
        );

        if !waitlisted.is_reply() && waitlisted.kind() != DispatchKind::Signal {
            // Trap explanation.
            let err = ErrorReplyReason::RemovedFromWaitlist;

            // Trap reply payload.
            let err_payload = err
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Error message is too large"));

            let trap_reply = ReplyMessage::system(message_id, err_payload, err);

            // Generate trap reply.
            if self.check_program_id(&waitlisted.source()) {
                let trap_dispatch =
                    trap_reply.into_stored_dispatch(program_id, waitlisted.source(), message_id);

                // Creating `GasNode` for the reply.
                Pallet::<T>::split(
                    waitlisted.id(),
                    trap_dispatch.id(),
                    trap_dispatch.is_reply(),
                );

                // Queueing dispatch.
                QueueOf::<T>::queue(trap_dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
            } else {
                // Sending trap reply to user, by depositing event.
                //
                // There is no reason to use `Pallet::<T>::send_user_message( .. )`,
                // because there is no need in reply in future, so no reason
                // and funds to pay mailbox rent for it.
                let trap_reply =
                    trap_reply.into_stored(program_id, waitlisted.source(), message_id);
                let trap_reply = trap_reply
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("Signal message sent to user"));

                // Depositing appropriate event.
                Pallet::<T>::deposit_event(Event::UserMessageSent {
                    message: trap_reply,
                    expiration: None,
                });
            }
        }

        // Consuming gas handler for waitlisted message.
        Pallet::<T>::consume_and_retrieve(waitlisted.id());

        if waitlisted.kind() == DispatchKind::Init {
            let origin = waitlisted.source();
            Self::process_failed_init(program_id, origin, true);
        }

        let gas = <T as Config>::WeightInfo::tasks_remove_from_waitlist().ref_time();
        log::trace!("Task gas: tasks_remove_from_waitlist = {gas}");

        gas
    }

    fn remove_paused_program(&mut self, _program_id: ProgramId) -> Gas {
        todo!("#646")
    }

    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> Gas {
        match Pallet::<T>::wake_dispatch(
            program_id,
            message_id,
            MessageWokenRuntimeReason::WakeCalled.into_reason(),
        ) {
            Some(dispatch) => {
                QueueOf::<T>::queue(dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

                let gas = <T as Config>::WeightInfo::tasks_wake_message().ref_time();
                log::trace!("Task gas: tasks_wake_message = {gas}");

                gas
            }
            None => {
                let gas = <T as Config>::WeightInfo::tasks_wake_message_no_wake().ref_time();
                log::trace!("Task gas: tasks_wake_message_no_wake = {gas}");

                gas
            }
        }
    }

    fn send_dispatch(&mut self, stashed_message_id: MessageId) -> Gas {
        // No validation required. If program doesn't exist, then NotExecuted appears.

        let (dispatch, hold_interval) = DispatchStashOf::<T>::take(stashed_message_id)
            .unwrap_or_else(|| unreachable!("Scheduler & Stash logic invalidated!"));

        // Charging locked gas for holding in dispatch stash.
        Pallet::<T>::charge_for_hold(dispatch.id(), hold_interval, StorageType::DispatchStash);

        QueueOf::<T>::queue(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

        let gas = <T as Config>::WeightInfo::tasks_send_dispatch().ref_time();
        log::trace!("Task gas: tasks_send_dispatch = {gas}");

        gas
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, to_mailbox: bool) -> Gas {
        // TODO: validate here destination and send error reply, if required.
        // Atm despite the fact that program may exist, message goes into mailbox / event.
        let (message, hold_interval) = DispatchStashOf::<T>::take(stashed_message_id)
            .map(|(dispatch, interval)| (dispatch.into_parts().1, interval))
            .unwrap_or_else(|| unreachable!("Scheduler & Stash logic invalidated!"));

        // Charge gas for message save.
        Pallet::<T>::charge_for_hold(message.id(), hold_interval, StorageType::DispatchStash);

        // Cast message type.
        let message = message
            .try_into()
            .unwrap_or_else(|_| unreachable!("Signal message sent to user"));
        Pallet::<T>::send_user_message_after_delay(message, to_mailbox);

        if to_mailbox {
            let gas = <T as Config>::WeightInfo::tasks_send_user_message_to_mailbox().ref_time();
            log::trace!("Task gas: tasks_send_user_message_to_mailbox = {gas}");

            gas
        } else {
            let gas = <T as Config>::WeightInfo::tasks_send_user_message().ref_time();
            log::trace!("Task gas: tasks_send_user_message = {gas}");

            gas
        }
    }

    fn remove_gas_reservation(
        &mut self,
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) -> Gas {
        let _slot = Self::remove_gas_reservation_impl(program_id, reservation_id);

        let gas = <T as Config>::WeightInfo::tasks_remove_gas_reservation().ref_time();
        log::trace!("Task gas: tasks_remove_gas_reservation = {gas}");

        gas
    }

    fn remove_resume_session(&mut self, _session_id: SessionId) -> Gas {
        0
    }
}
