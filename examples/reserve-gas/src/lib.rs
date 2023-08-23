// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub const RESERVATION_AMOUNT: u64 = 50_000_000;
pub const REPLY_FROM_RESERVATION_PAYLOAD: &[u8; 5] = b"Hello";

#[derive(Debug, Encode, Decode)]
pub enum InitAction {
    Normal(Vec<(u64, u32)>),
    Wait,
    CheckArgs { mailbox_threshold: u64 },
    FreshReserveUnreserve,
}

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Unreserve,
    Exit,
    ReplyFromReservation,
    AddReservationToList(GasAmount, BlockCount),
    ConsumeReservationsFromList,
    RunInifitely,
    SendFromReservationAndUnreserve,
}

#[derive(Debug, Encode, Decode)]
pub enum ReplyAction {
    Panic,
    Exit,
}

pub type GasAmount = u64;
pub type BlockCount = u32;

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use crate::InitAction;
    use alloc::vec;
    use gtest::{Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send(
            0,
            InitAction::Normal(vec![
                // orphan reservation; will be removed automatically
                (50_000, 3),
                // must be cleared during `gr_exit`
                (25_000, 5),
            ]),
        );
        assert!(!res.main_failed());
    }
}
