// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    manager::ExtManager, weights::WeightInfo, Config, CurrencyOf, DispatchStashOf, Event, Pallet,
    ProgramStorageOf, QueueOf, WaitlistOf,
};
use alloc::string::ToString;
use common::{
    event::{
        MessageWokenRuntimeReason, MessageWokenSystemReason, ProgramChangeKind, RuntimeReason,
        SystemReason, UserMessageReadSystemReason,
    },
    paused_program_storage::SessionId,
    scheduler::*,
    storage::*,
    Origin, PausedProgramStorage, Program, ProgramStorage,
};
use core::{cmp, convert::TryInto};
use frame_support::traits::{Currency, ExistenceRequirement};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::{DispatchKind, ReplyMessage},
};
use gear_core_errors::{SimpleReplyError, SimpleSignalError};
use sp_runtime::traits::Zero;

pub fn get_maximum_task_gas<T: Config>(task: &ScheduledTask<T::AccountId>) -> Gas {
    use ScheduledTask::*;

    match task {
        PauseProgram(_) => 0,
        RemoveCode(_) => todo!("#646"),
        RemoveFromMailbox(_, _) => 0,
        RemoveFromWaitlist(_, _) => 0,
        RemovePausedProgram(_) => todo!("#646"),
        WakeMessage(_, _) => cmp::max(
            <T as Config>::WeightInfo::tasks_wake_message().ref_time(),
            <T as Config>::WeightInfo::tasks_wake_message_no_wake().ref_time(),
        ),
        SendDispatch(_) => <T as Config>::WeightInfo::tasks_send_dispatch().ref_time(),
        SendUserMessage { .. } => core::cmp::max(
            <T as Config>::WeightInfo::tasks_send_user_message_to_mailbox().ref_time(),
            <T as Config>::WeightInfo::tasks_send_user_message().ref_time(),
        ),
        RemoveGasReservation(_, _) => {
            <T as Config>::WeightInfo::tasks_remove_gas_reservation().ref_time()
        }
        RemoveResumeSession(_) => {
            <T as Config>::WeightInfo::tasks_remove_resume_session().ref_time()
        }
    }
}

impl<T: Config> TaskHandler<T::AccountId> for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn pause_program(&mut self, program_id: ProgramId) -> Gas {
        log::debug!("pause_program; id = {:?}", program_id);

        let program = ProgramStorageOf::<T>::get_program(program_id)
            .unwrap_or_else(|| unreachable!("Program to pause not found."));

        let Some(init_message_id) = program.is_uninitialized() else {
            // pause initialized program
            let gas_reservation_map = ProgramStorageOf::<T>::pause_program(program_id, Pallet::<T>::block_number())
                .unwrap_or_else(|e| unreachable!("Failed to pause program: {:?}", e));

            // clean wait list from the messages
            let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();
            WaitlistOf::<T>::drain_key(program_id).for_each(|entry| {
                let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());

                QueueOf::<T>::queue(message)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
            });

            Self::remove_gas_reservation_map(program_id, gas_reservation_map);
            Pallet::<T>::deposit_event(Event::ProgramChanged {
                id: program_id,
                change: ProgramChangeKind::Paused,
            });

            return 0;
        };

        // terminate uninitialized program

        // clean wait list from the messages
        let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();
        let origin = WaitlistOf::<T>::drain_key(program_id)
            .fold(None, |maybe_origin, entry| {
                let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());
                let result = match maybe_origin {
                    Some(_) => maybe_origin,
                    None if init_message_id == message.message().id() => {
                        Some(message.message().source())
                    }
                    _ => None,
                };

                QueueOf::<T>::queue(message)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

                result
            })
            .unwrap_or_else(|| unreachable!("Failed to find init-message."));

        // set program status to Terminated
        let gas_reservations =
            ProgramStorageOf::<T>::update_program_if_active(program_id, |p, _bn| {
                let gas_reservations = match p {
                    Program::Active(program) => core::mem::take(&mut program.gas_reservation_map),
                    _ => unreachable!("Action executed only for active program"),
                };

                *p = Program::Terminated(origin);

                gas_reservations
            })
            .unwrap_or_else(|e| {
                unreachable!(
                    "Program terminated status may only be set to an existing active program: {:?}",
                    e,
                );
            });

        // remove program's resources
        Self::remove_gas_reservation_map(program_id, gas_reservations);
        ProgramStorageOf::<T>::remove_program_pages(program_id);
        ProgramStorageOf::<T>::waiting_init_remove(program_id);

        let event = Event::ProgramChanged {
            id: program_id,
            change: ProgramChangeKind::Terminated,
        };

        // transfer program's balance to the origin
        let program_id = <T::AccountId as Origin>::from_origin(program_id.into_origin());

        let balance = CurrencyOf::<T>::free_balance(&program_id);
        let destination = Pallet::<T>::inheritor_for(origin);
        let destination = <T::AccountId as Origin>::from_origin(destination.into_origin());

        if !balance.is_zero() {
            CurrencyOf::<T>::transfer(
                &program_id,
                &destination,
                balance,
                ExistenceRequirement::AllowDeath,
            )
            .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
        }

        Pallet::<T>::deposit_event(event);

        0
    }

    fn remove_code(&mut self, _code_id: CodeId) -> Gas {
        todo!("#646")
    }

    fn remove_from_mailbox(&mut self, user_id: T::AccountId, message_id: MessageId) -> Gas {
        // Read reason.
        let reason = UserMessageReadSystemReason::OutOfRent.into_reason();

        // Trap explanation.
        let err = SimpleReplyError::OutOfRent;

        // Trap reply payload.
        let err_payload = err
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap_or_else(|_| unreachable!("Error message is too large"));

        // Creating reply message.
        //
        // # Safety
        //
        // 1. The `message_id` has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let message = ReplyMessage::system(message_id, err_payload, err);

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

        0
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
            SimpleSignalError::RemovedFromWaitlist,
        );

        if !waitlisted.is_reply() && waitlisted.kind() != DispatchKind::Signal {
            // Trap explanation.
            let err = SimpleReplyError::OutOfRent;

            // Trap reply payload.
            let err_payload = err
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Error message is too large"));

            // Creating reply message.
            //
            // # Safety
            //
            // 1. The dispatch.id() has already been checked
            // 2. This reply message is generated by our system
            //
            // So, the message id of this reply message will not be duplicated.
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

                // Depositing appropriate event.
                Pallet::<T>::deposit_event(Event::UserMessageSent {
                    message: trap_reply,
                    expiration: None,
                });
            }
        }

        // Consuming gas handler for waitlisted message.
        Pallet::<T>::consume_and_retrieve(waitlisted.id());

        0
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

                <T as Config>::WeightInfo::tasks_wake_message().ref_time()
            }
            None => <T as Config>::WeightInfo::tasks_wake_message_no_wake().ref_time(),
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

        <T as Config>::WeightInfo::tasks_send_dispatch().ref_time()
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, to_mailbox: bool) -> Gas {
        // TODO: validate here destination and send error reply, if required.
        // Atm despite the fact that program may exist, message goes into mailbox / event.
        let (message, hold_interval) = DispatchStashOf::<T>::take(stashed_message_id)
            .map(|(dispatch, interval)| (dispatch.into_parts().1, interval))
            .unwrap_or_else(|| unreachable!("Scheduler & Stash logic invalidated!"));

        // Charge gas for message save.
        Pallet::<T>::charge_for_hold(message.id(), hold_interval, StorageType::DispatchStash);

        Pallet::<T>::send_user_message_after_delay(message, to_mailbox);

        match to_mailbox {
            true => <T as Config>::WeightInfo::tasks_send_user_message_to_mailbox(),
            false => <T as Config>::WeightInfo::tasks_send_user_message(),
        }
        .ref_time()
    }

    fn remove_gas_reservation(
        &mut self,
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) -> Gas {
        let _slot = Self::remove_gas_reservation_impl(program_id, reservation_id);

        <T as Config>::WeightInfo::tasks_remove_gas_reservation().ref_time()
    }

    fn remove_resume_session(&mut self, session_id: SessionId) -> Gas {
        log::debug!("Execute task to remove resume session with session_id = {session_id}");
        ProgramStorageOf::<T>::remove_resume_session(session_id)
            .unwrap_or_else(|e| unreachable!("ProgramStorage corrupted! {:?}", e));

        <T as Config>::WeightInfo::tasks_remove_resume_session().ref_time()
    }
}
