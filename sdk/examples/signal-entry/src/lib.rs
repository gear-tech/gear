// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

// We can't depend on gstd because it declares panic handler, so we just use gcore.
use gcore::errors::SignalCode;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
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
    ForbiddenAction,
    SaveSignal(SignalCode),
    ExceedMemory,
    ExceedStackLimit,
    UnreachableInstruction,
    InvalidDebugCall,
    UnrecoverableExt,
    IncorrectFree,
    WaitWithoutSendingMessage,
    MemoryAccess,
}

pub const WAIT_AND_RESERVE_WITH_PANIC_GAS: u64 = 10_000_000_000;

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;

#[cfg(test)]
mod tests {
    use crate::HandleAction;
    use gtest::{Log, Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn signal_can_be_sent() {
        let system = System::new();
        system.init_logger();

        let user_id = DEFAULT_USER_ALICE;
        let program = Program::current(&system);

        // Initialize program
        program.send_bytes(user_id, b"init_program");
        system.run_next_block();

        // Make program panic
        let msg_id = program.send(user_id, HandleAction::Panic);
        let res = system.run_next_block();

        // Checking signal executed successfully by checking if there are failed messages.
        assert_eq!(res.failed.len(), 1);
        assert!(res.failed.contains(&msg_id));
        assert!(res.not_executed.is_empty());

        // Signal sends user message
        let log = Log::builder().dest(user_id).payload(b"handle_signal");
        assert!(res.contains(&log));
        let mailbox = system.get_mailbox(user_id);
        assert!(mailbox.contains(&log));
    }
}
