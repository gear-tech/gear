// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum Action {
    Reserve { amount: u64, duration: u32 },
    SendMessageFromReservation { gas_amount: u64 },
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{msg, prelude::*, Reservations};
    use parity_scale_codec::{Decode, Encode};

    static mut RESERVATIONS: Reservations = Reservations::new();

    #[no_mangle]
    extern "C" fn handle() {
        let action: Action = msg::load().expect("Failed to load message");

        unsafe {
            match action {
                Action::Reserve { amount, duration } => {
                    RESERVATIONS
                        .reserve(amount, duration)
                        .expect("Failed to reserve gas");
                }
                Action::SendMessageFromReservation { gas_amount } => {
                    let reservation = RESERVATIONS.try_take_reservation(gas_amount);
                    if let Some(reservation) = reservation {
                        msg::send_bytes_from_reservation(reservation.id(), msg::source(), [], 0)
                            .expect("Failed to send message from reservation");
                    } else {
                        panic!("Reservation not found");
                    }
                }
            }
        }
    }
}
