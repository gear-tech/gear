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

use crate::ids::{ActorId, MessageId, ReservationId};
use gsys::Gas;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Alias for ScheduledTask used in vara-runtime, generic across AccountId used.
pub type VaraScheduledTask<AccountId> = ScheduledTask<AccountId, MessageId, bool>;

/// Scheduled task sense and required data for processing action.
///
/// CAUTION: NEVER ALLOW `ScheduledTask` BE A BIG DATA.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ScheduledTask<RFM, SD, SUM> {
    /// Remove message from mailbox as out of rent one.
    RemoveFromMailbox(RFM, MessageId),

    /// Remove message from waitlist as out of rent one.
    RemoveFromWaitlist(ActorId, MessageId),

    // Time chained section.
    // -----
    /// Delayed wake of the message at concrete block.
    WakeMessage(ActorId, MessageId),

    /// Delayed message to program sending.
    ///
    /// The message itself stored in DispatchStash.
    SendDispatch(SD),

    /// Delayed message to user sending.
    ///
    /// The message itself stored in DispatchStash.
    SendUserMessage {
        /// What message to send.
        message_id: MessageId,
        /// Should it be inserted into users mailbox.
        to_mailbox: SUM,
    },

    /// Remove gas reservation.
    RemoveGasReservation(ActorId, ReservationId),
}

impl<RFM, SD, SUM> ScheduledTask<RFM, SD, SUM> {
    /// Processing function of current task with given handler.
    pub fn process_with(self, handler: &mut impl TaskHandler<RFM, SD, SUM>) -> Gas {
        use ScheduledTask::*;

        match self {
            RemoveFromMailbox(user_id, message_id) => {
                handler.remove_from_mailbox(user_id, message_id)
            }
            RemoveFromWaitlist(program_id, message_id) => {
                handler.remove_from_waitlist(program_id, message_id)
            }
            WakeMessage(program_id, message_id) => handler.wake_message(program_id, message_id),
            SendDispatch(message_id) => handler.send_dispatch(message_id),
            SendUserMessage {
                message_id,
                to_mailbox,
            } => handler.send_user_message(message_id, to_mailbox),
            RemoveGasReservation(program_id, reservation_id) => {
                handler.remove_gas_reservation(program_id, reservation_id)
            }
        }
    }
}

/// Task handler trait for dealing with required tasks.
pub trait TaskHandler<RFM, SD, SUM> {
    // Rent charging section.
    // -----
    /// Remove from mailbox action.
    fn remove_from_mailbox(&mut self, user_id: RFM, message_id: MessageId) -> Gas;
    /// Remove from waitlist action.
    fn remove_from_waitlist(&mut self, program_id: ActorId, message_id: MessageId) -> Gas;

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
}

#[test]
fn task_encoded_size() {
    // We will force represent task with no more then 2^8 (256) bytes.
    const MAX_SIZE: usize = 256;

    // For example we will take `AccountId` = `ActorId` from `gear_core`.
    assert!(VaraScheduledTask::<ActorId>::max_encoded_len() <= MAX_SIZE);
}
