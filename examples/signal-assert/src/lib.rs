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

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

// #[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{debug, errors::SignalCode, msg, prelude::*};

    static mut SIGNAL: Option<SignalCode> = None;

    #[no_mangle]
    extern "C" fn init() {
        let signal_received: SignalCode = msg::load().unwrap();
        let signal_saved = unsafe { &mut SIGNAL };

        *signal_saved = Some(signal_received);

        debug!(
            "init: signal_received={:?}, signal_saved={:?}",
            signal_received, signal_saved
        );
    }

    #[no_mangle]
    extern "C" fn handle() {
        unimplemented!("This program is not supposed to be executed directly")
    }

    #[no_mangle]
    extern "C" fn handle_signal() {
        let signal_received = msg::signal_code()
            .expect("Incorrect call")
            .expect("Unsupported code");

        let signal_saved: Option<SignalCode> = unsafe { SIGNAL };

        if let Some(signal_saved) = signal_saved {
            assert_eq!(signal_received, signal_saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use gstd::errors::{SignalCode, SimpleExecutionError};
    use gtest::{Program, System};

    fn send_and_assert_signals_eq(signal1: SignalCode, signal2: SignalCode) {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send(0, signal1);
        assert!(!res.main_failed(), "saving signal {:?} failed", signal1);

        let res = program.send_signal(0, signal2);
        assert!(!res.main_failed(), "sending signal {:?} failed", signal2);
    }

    #[test]
    #[should_panic(expected = "sending signal")]
    fn demo_fails_on_wrong_signal() {
        send_and_assert_signals_eq(
            SignalCode::RemovedFromWaitlist,
            SignalCode::Execution(SimpleExecutionError::BackendError),
        )
    }

    #[test]
    fn all_signals() {
        let signals: Vec<SignalCode> = vec![
            SignalCode::RemovedFromWaitlist,
            SimpleExecutionError::BackendError.into(),
            SimpleExecutionError::MemoryOverflow.into(),
            SimpleExecutionError::RanOutOfGas.into(),
            SimpleExecutionError::UnreachableInstruction.into(),
            SimpleExecutionError::UserspacePanic.into(),
        ];

        for signal in signals {
            send_and_assert_signals_eq(signal, signal)
        }
    }
}
