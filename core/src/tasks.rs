// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! The module provides primitives for all available regular or time-dependent tasks.

use crate::ids::{ActorId, CodeId, MessageId, ReservationId};
use gsys::Gas;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Alias for ScheduledTask used in vara-runtime, generic across AccountId used.
pub type VaraScheduledTask<AccountId> = ScheduledTask<AccountId, MessageId, bool>;

/// Scheduled task sense and required data for processing action.
///
/// CAUTION: NEVER ALLOW `ScheduledTask` BE A BIG DATA.
/// To avoid redundant migrations only append new variant(s) to the enum
/// with an explicit corresponding scale codec index.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ScheduledTask<RFM, SD, SUM> {
    // Rent charging section.
    // -----
    /// Pause program as out of rent one.
    #[codec(index = 0)]
    PauseProgram(ActorId),

    /// Remove code from code storage as out of rent one.
    #[codec(index = 1)]
    RemoveCode(CodeId),

    /// Remove message from mailbox as out of rent one.
    #[codec(index = 2)]
    RemoveFromMailbox(RFM, MessageId),

    /// Remove message from waitlist as out of rent one.
    #[codec(index = 3)]
    RemoveFromWaitlist(ActorId, MessageId),

    /// Remove paused program as dead one (issue #1014).
    #[codec(index = 4)]
    RemovePausedProgram(ActorId),

    // Time chained section.
    // -----
    /// Delayed wake of the message at concrete block.
    #[codec(index = 5)]
    WakeMessage(ActorId, MessageId),

    /// Delayed message to program sending.
    ///
    /// The message itself stored in DispatchStash.
    #[codec(index = 6)]
    SendDispatch(SD),

    /// Delayed message to user sending.
    ///
    /// The message itself stored in DispatchStash.
    #[codec(index = 7)]
    SendUserMessage {
        /// What message to send.
        message_id: MessageId,
        /// Should it be inserted into users mailbox.
        to_mailbox: SUM,
    },

    /// Remove gas reservation.
    #[codec(index = 8)]
    RemoveGasReservation(ActorId, ReservationId),

    /// Remove resume program session.
    #[codec(index = 9)]
    #[deprecated = "Paused program storage was removed in pallet-gear-program"]
    RemoveResumeSession(u32),
}

impl<RFM, SD, SUM> ScheduledTask<RFM, SD, SUM> {
    /// Processing function of current task with given handler.
    pub fn process_with(self, handler: &mut impl TaskHandler<RFM, SD, SUM>) -> Gas {
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
            #[allow(deprecated)]
            RemoveResumeSession(session_id) => handler.remove_resume_session(session_id),
        }
    }
}

/// Task handler trait for dealing with required tasks.
pub trait TaskHandler<RFM, SD, SUM> {
    // Rent charging section.
    // -----
    /// Pause program action.
    fn pause_program(&mut self, program_id: ActorId) -> Gas;
    /// Remove code action.
    fn remove_code(&mut self, code_id: CodeId) -> Gas;
    /// Remove from mailbox action.
    fn remove_from_mailbox(&mut self, user_id: RFM, message_id: MessageId) -> Gas;
    /// Remove from waitlist action.
    fn remove_from_waitlist(&mut self, program_id: ActorId, message_id: MessageId) -> Gas;
    /// Remove paused program action.
    fn remove_paused_program(&mut self, program_id: ActorId) -> Gas;

    // Time chained section.
    // -----
    /// Wake message action.
    fn wake_message(&mut self, program_id: ActorId, message_id: MessageId) -> Gas;

    /// Send delayed message to program action.
    fn send_dispatch(&mut self, stashed_message_id: SD) -> Gas;

    /// Send delayed message to user action.
    fn send_user_message(&mut self, stashed_message_id: MessageId, to_mailbox: SUM) -> Gas;

    /// Remove gas reservation action.
    fn remove_gas_reservation(&mut self, program_id: ActorId, reservation_id: ReservationId)
    -> Gas;

    /// Remove data created by resume program session.
    fn remove_resume_session(&mut self, session_id: u32) -> Gas;
}

#[test]
fn task_encoded_size() {
    // We will force represent task with no more then 2^8 (256) bytes.
    const MAX_SIZE: usize = 256;

    // For example we will take `AccountId` = `ActorId` from `gear_core`.
    assert!(VaraScheduledTask::<ActorId>::max_encoded_len() <= MAX_SIZE);
}
