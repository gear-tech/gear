// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

use crate::{paused_program_storage::SessionId, Gas};
use gear_core::ids::{CodeId, MessageId, ProgramId, ReservationId};
use sp_runtime::{
    codec::{self, Decode, Encode, MaxEncodedLen},
    scale_info::{self, TypeInfo},
};

/// Scheduled task sense and required data for processing action.
///
/// CAUTION: NEVER ALLOW `ScheduledTask<AccountId>` BE A BIG DATA.
/// To avoid redundant migrations only append new variant(s) to the enum
/// with an explicit corresponding scale codec index.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum ScheduledTask<AccountId> {
    // Rent charging section.
    // -----
    /// Pause program as out of rent one.
    #[codec(index = 0)]
    PauseProgram(ProgramId),

    /// Remove code from code storage as out of rent one.
    #[codec(index = 1)]
    RemoveCode(CodeId),

    /// Remove message from mailbox as out of rent one.
    #[codec(index = 2)]
    RemoveFromMailbox(AccountId, MessageId),

    /// Remove message from waitlist as out of rent one.
    #[codec(index = 3)]
    RemoveFromWaitlist(ProgramId, MessageId),

    /// Remove paused program as dead one (issue #1014).
    #[codec(index = 4)]
    RemovePausedProgram(ProgramId),

    // Time chained section.
    // -----
    /// Delayed wake of the message at concrete block.
    #[codec(index = 5)]
    WakeMessage(ProgramId, MessageId),

    /// Delayed message to program sending.
    ///
    /// The message itself stored in DispatchStash.
    #[codec(index = 6)]
    SendDispatch(MessageId),

    /// Delayed message to user sending.
    ///
    /// The message itself stored in DispatchStash.
    #[codec(index = 7)]
    SendUserMessage {
        message_id: MessageId,
        to_mailbox: bool,
    },

    /// Remove gas reservation.
    #[codec(index = 8)]
    RemoveGasReservation(ProgramId, ReservationId),

    /// Remove resume program session.
    #[codec(index = 9)]
    RemoveResumeSession(SessionId),
}

impl<AccountId> ScheduledTask<AccountId> {
    pub fn process_with(self, handler: &mut impl TaskHandler<AccountId>) -> Gas {
        use ScheduledTask::*;

        match self {
            PauseProgram(program_id) => handler.pause_program(program_id),
            RemoveCode(code_id) => handler.remove_code(code_id),
            RemoveFromMailbox(user_id, message_id) => {
                handler.remove_from_mailbox(user_id, message_id)
            }
            RemoveFromWaitlist(program_id, message_id) => {
                handler.remove_from_waitlist(program_id, message_id)
            }
            RemovePausedProgram(program_id) => handler.remove_paused_program(program_id),
            WakeMessage(program_id, message_id) => handler.wake_message(program_id, message_id),
            SendDispatch(message_id) => handler.send_dispatch(message_id),
            SendUserMessage {
                message_id,
                to_mailbox,
            } => handler.send_user_message(message_id, to_mailbox),
            RemoveGasReservation(program_id, reservation_id) => {
                handler.remove_gas_reservation(program_id, reservation_id)
            }
            RemoveResumeSession(session_id) => handler.remove_resume_session(session_id),
        }
    }
}

/// Task handler trait for dealing with required tasks.
pub trait TaskHandler<AccountId> {
    // Rent charging section.
    // -----
    /// Pause program action.
    fn pause_program(&mut self, program_id: ProgramId) -> Gas;
    /// Remove code action.
    fn remove_code(&mut self, code_id: CodeId) -> Gas;
    /// Remove from mailbox action.
    fn remove_from_mailbox(&mut self, user_id: AccountId, message_id: MessageId) -> Gas;
    /// Remove from waitlist action.
    fn remove_from_waitlist(&mut self, program_id: ProgramId, message_id: MessageId) -> Gas;
    /// Remove paused program action.
    fn remove_paused_program(&mut self, program_id: ProgramId) -> Gas;

    // Time chained section.
    // -----
    /// Wake message action.
    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> Gas;

    // Send delayed message to program action.
    fn send_dispatch(&mut self, stashed_message_id: MessageId) -> Gas;

    // Send delayed message to user action.
    fn send_user_message(&mut self, stashed_message_id: MessageId, to_mailbox: bool) -> Gas;

    /// Remove gas reservation action.
    fn remove_gas_reservation(
        &mut self,
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) -> Gas;

    /// Remove data created by resume program session.
    fn remove_resume_session(&mut self, session_id: SessionId) -> Gas;
}

#[test]
fn task_encoded_size() {
    // We will force represent task with no more then 2^8 (256) bytes.
    const MAX_SIZE: usize = 256;

    // For example we will take `AccountId` = `ProgramId` from `gear_core`.
    type AccountId = ProgramId;

    assert!(ScheduledTask::<AccountId>::max_encoded_len() <= MAX_SIZE);
}
