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
use gstd::{msg, prelude::*, ReservationId};

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
    SendToProgram([u8; 32]),
    SendToProgramDelayed([u8; 32]),
    ReplyToUser,
    ReplyToUserDelayed,
    ReplyToProgram { pid: [u8; 32], user: [u8; 32] },
    ReplyToProgramStep2([u8; 32]),
    ReplyToProgramDelayed { pid: [u8; 32], user: [u8; 32] },
    ReplyToProgramDelayedStep2([u8; 32]),
    ReceiveFromProgram,
    ReceiveFromProgramDelayed,
}

#[derive(Debug, Encode, Decode)]
pub enum ReplyAction {
    Receive([u8; 32]),
    ReceiveDelayed([u8; 32]),
}

#[no_mangle]
unsafe extern "C" fn init() {}

#[no_mangle]
unsafe extern "C" fn handle() {
    let action: HandleAction = msg::load().unwrap();
    match action {
        HandleAction::SendToUser => {
            let id = ReservationId::reserve(3_000_000_000, 50).unwrap();
            msg::send_from_reservation(id, msg::source(), 0, 500).unwrap();
        }
        HandleAction::SendToUserDelayed => {
            let id = ReservationId::reserve(4_000_000_000, 60).unwrap();
            msg::send_delayed_from_reservation(id, msg::source(), 0, 600, 1).unwrap();
        }
        HandleAction::SendToProgram(pid) => {
            let id = ReservationId::reserve(5_000_000_000, 70).unwrap();
            msg::send_from_reservation(id, pid.into(), HandleAction::ReceiveFromProgram, 700)
                .unwrap();
        }
        HandleAction::SendToProgramDelayed(pid) => {
            let id = ReservationId::reserve(6_000_000_000, 80).unwrap();
            msg::send_delayed_from_reservation(
                id,
                pid.into(),
                HandleAction::ReceiveFromProgramDelayed,
                800,
                1,
            )
            .unwrap();
        }
        HandleAction::ReplyToUser => {
            let id = ReservationId::reserve(7_000_000_000, 90).unwrap();
            msg::reply_from_reservation(id, "reply_to_user", 900).unwrap();
        }
        HandleAction::ReplyToUserDelayed => {
            let id = ReservationId::reserve(8_000_000_000, 100).unwrap();
            msg::reply_delayed_from_reservation(id, "reply_to_user_delayed", 1000, 1).unwrap();
        }
        HandleAction::ReplyToProgram { pid, user } => {
            msg::send(pid.into(), HandleAction::ReplyToProgramStep2(user), 900).unwrap();
        }
        HandleAction::ReplyToProgramStep2(user) => {
            let id = ReservationId::reserve(7_000_000_000, 90).unwrap();
            msg::reply_from_reservation(id, ReplyAction::Receive(user), 900).unwrap();
        }
        HandleAction::ReplyToProgramDelayed { pid, user } => {
            msg::send(
                pid.into(),
                HandleAction::ReplyToProgramDelayedStep2(user),
                1000,
            )
            .unwrap();
        }
        HandleAction::ReplyToProgramDelayedStep2(user) => {
            let id = ReservationId::reserve(8_000_000_000, 100).unwrap();
            msg::reply_delayed_from_reservation(id, ReplyAction::ReceiveDelayed(user), 1000, 1)
                .unwrap();
        }
        HandleAction::ReceiveFromProgram => {
            assert_eq!(msg::value(), 700);
        }
        HandleAction::ReceiveFromProgramDelayed => {
            assert_eq!(msg::value(), 800);
        }
    }
}

#[no_mangle]
unsafe extern "C" fn handle_reply() {
    let action: ReplyAction = msg::load().unwrap();
    match action {
        ReplyAction::Receive(user) => {
            assert_eq!(msg::value(), 900);
            msg::send(user.into(), "reply", 900).unwrap();
        }
        ReplyAction::ReceiveDelayed(user) => {
            assert_eq!(msg::value(), 1000);
            msg::send(user.into(), "reply_delayed", 1000).unwrap();
        }
    }
}
