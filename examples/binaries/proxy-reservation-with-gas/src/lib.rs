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
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct InputArgs {
    pub destination: gstd::ActorId,
    pub delay: u32,
    pub reservation_amount: u64,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::InputArgs;
    use gstd::{msg, ActorId, ReservationId};

    static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);
    static mut DELAY: u32 = 0;
    static mut RESERVATION_AMOUNT: u64 = 0;

    #[no_mangle]
    extern "C" fn handle() {
        let gas_limit: u64 = msg::load().expect("Failed to decode `gas_limit: u64'");
        let reservation_id = ReservationId::reserve(unsafe { RESERVATION_AMOUNT }, 80)
            .expect("Failed to reserve gas");
        msg::send_delayed_from_reservation(
            reservation_id,
            unsafe { DESTINATION },
            b"proxied message",
            msg::value(),
            unsafe { DELAY },
        )
        .expect("Failed to proxy message");
    }

    #[no_mangle]
    extern "C" fn init() {
        let args: InputArgs = msg::load().expect("Failed to decode `InputArgs'");
        unsafe {
            DESTINATION = args.destination;
            DELAY = args.delay;
            RESERVATION_AMOUNT = args.reservation_amount;
        }
    }
}
