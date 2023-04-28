// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    SendToUser,
    SendToUserDelayed,
    SendToProgram { pid: [u8; 32], user: [u8; 32] },
    SendToProgramDelayed { pid: [u8; 32], user: [u8; 32] },
    ReplyToUser,
    ReplyToUserDelayed,
    ReplyToProgram { pid: [u8; 32], user: [u8; 32] },
    ReplyToProgramStep2([u8; 32]),
    ReplyToProgramDelayed { pid: [u8; 32], user: [u8; 32] },
    ReplyToProgramDelayedStep2([u8; 32]),
    ReceiveFromProgram([u8; 32]),
    ReceiveFromProgramDelayed([u8; 32]),
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{msg, prelude::*, ReservationId};

    #[derive(Debug, Encode, Decode)]
    pub enum ReplyAction {
        Receive([u8; 32]),
        ReceiveDelayed([u8; 32]),
    }

    #[no_mangle]
    extern "C" fn handle() {
        let action: HandleAction = msg::load().expect("Failed to load handle payload");
        match action {
            HandleAction::SendToUser => {
                let id = ReservationId::reserve(3_000_000_000, 50).expect("Failed to reserve gas");
                msg::send_bytes_from_reservation(id, msg::source(), b"send_to_user", 500)
                    .expect("Failed to send message");
            }
            HandleAction::SendToUserDelayed => {
                let id = ReservationId::reserve(4_000_000_000, 60).expect("Failed to reserve gas");
                msg::send_bytes_delayed_from_reservation(
                    id,
                    msg::source(),
                    b"send_to_user_delayed",
                    600,
                    1,
                )
                .expect("Failed to send message");
            }
            HandleAction::SendToProgram { pid, user } => {
                let id = ReservationId::reserve(5_000_000_000, 70).expect("Failed to reserve gas");
                msg::send_from_reservation(
                    id,
                    pid.into(),
                    HandleAction::ReceiveFromProgram(user),
                    700,
                )
                .expect("Failed to send message");
            }
            HandleAction::SendToProgramDelayed { pid, user } => {
                let id = ReservationId::reserve(6_000_000_000, 80).expect("Failed to reserve gas");
                msg::send_delayed_from_reservation(
                    id,
                    pid.into(),
                    HandleAction::ReceiveFromProgramDelayed(user),
                    800,
                    1,
                )
                .expect("Failed to send message");
            }
            HandleAction::ReplyToUser => {
                let id = ReservationId::reserve(7_000_000_000, 90).expect("Failed to reserve gas");
                msg::reply_bytes_from_reservation(id, b"reply_to_user", 900)
                    .expect("Failed to send message");
            }
            HandleAction::ReplyToUserDelayed => {
                let id = ReservationId::reserve(8_000_000_000, 100).expect("Failed to reserve gas");
                msg::reply_bytes_delayed_from_reservation(id, b"reply_to_user_delayed", 1000, 1)
                    .expect("Failed to send message");
            }
            HandleAction::ReplyToProgram { pid, user } => {
                msg::send(pid.into(), HandleAction::ReplyToProgramStep2(user), 900)
                    .expect("Failed to reserve gas");
            }
            HandleAction::ReplyToProgramStep2(user) => {
                let id = ReservationId::reserve(7_000_000_000, 90).expect("Failed to reserve gas");
                msg::reply_from_reservation(id, ReplyAction::Receive(user), 900)
                    .expect("Failed to reply");
            }
            HandleAction::ReplyToProgramDelayed { pid, user } => {
                msg::send(
                    pid.into(),
                    HandleAction::ReplyToProgramDelayedStep2(user),
                    1000,
                )
                .expect("Failed to send message");
            }
            HandleAction::ReplyToProgramDelayedStep2(user) => {
                let id = ReservationId::reserve(8_000_000_000, 100).expect("Failed to reserve gas");
                msg::reply_delayed_from_reservation(id, ReplyAction::ReceiveDelayed(user), 1000, 1)
                    .expect("Failed to reply");
            }
            HandleAction::ReceiveFromProgram(user) => {
                assert_eq!(msg::value(), 700);
                msg::send_bytes(user.into(), b"receive_from_program", 700)
                    .expect("Failed to send message");
            }
            HandleAction::ReceiveFromProgramDelayed(user) => {
                assert_eq!(msg::value(), 800);
                msg::send_bytes(user.into(), b"receive_from_program_delayed", 800)
                    .expect("Failed to send message");
            }
        }
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        let action: ReplyAction = msg::load().expect("Failed to load handle payload");
        match action {
            ReplyAction::Receive(user) => {
                assert_eq!(msg::value(), 900);
                msg::send_bytes(user.into(), b"reply", 900).expect("Failed to send message");
            }
            ReplyAction::ReceiveDelayed(user) => {
                assert_eq!(msg::value(), 1000);
                msg::send_bytes(user.into(), b"reply_delayed", 1000)
                    .expect("Failed to send message");
            }
        }
    }
}
