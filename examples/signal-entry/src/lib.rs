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

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Simple,
    Wait,
    WaitAndPanic,
    WaitAndReserveWithPanic,
    WaitAndExit,
    WaitWithReserveAmountAndPanic(u64),
    Panic,
    Exit,
    Accumulate,
    OutOfGas,
    PanicInSignal,
    AcrossWaits,
    ZeroReserve,
    ForbiddenCallInSignal([u8; 32]),
}

pub const WAIT_AND_RESERVE_WITH_PANIC_GAS: u64 = 10_000_000_000;

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use gstd::errors::{SignalCode, SimpleExecutionError};
    use gtest::{Program, System};

    #[test]
    fn signal_can_be_sent() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let signal_code: SignalCode = SimpleExecutionError::UserspacePanic.into();
        let res = program.send_signal(0, signal_code);
        assert!(!res.main_failed());
    }
}
