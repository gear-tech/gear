// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

mod error;
mod log;
mod mailbox;
mod manager;
mod program;
mod system;

pub use crate::log::{CoreLog, Log, RunResult};
pub use codec;
pub use error::{Result, TestError};
pub use program::{calculate_program_id, Gas, Program, WasmProgram};
pub use system::System;

pub(crate) use constants::*;

/// Module containing constants of Gear protocol.
pub mod constants {
    /* Constant types */

    /// Numeric type representing value in Gear protocol.
    pub type Value = u128;

    /// Numeric type representing gas in Gear protocol.
    pub type Gas = u64;

    /// Numeric type representing blocks in Gear protocol.
    pub type Block = u32;

    /* Currency-related constants */

    /// Value per token.
    pub const UNITS: Value = 1_000_000_000_000;
    /// Minimal amount of value able to be sent. Defines accounts existence
    /// requirement.
    pub const EXISTENTIAL_DEPOSIT: Value = 10 * UNITS;
    /// Value per gas.
    pub const VALUE_PER_GAS: Value = 25;
    /// Duration of one epoch.
    pub const EPOCH_DURATION_IN_BLOCKS: Block = 600;

    /* Storage-related constants */
    // TODO: use proper weights of db accesses (#3509).

    /// Minimal amount of gas required to be inserted into Mailbox.
    pub const MAILBOX_THRESHOLD: Gas = 3_000;
    /// Extra amount of blocks must be reserved for storing in storage.
    pub const RESERVE_FOR: Block = 1;
    /// Cost of read access into storage.
    pub const READ_COST: Gas = 25;
    /// Per-byte extra cost of read access into storage.
    pub const READ_PER_BYTE_COST: Gas = 10;
    /// Cost of write access into storage.
    pub const WRITE_COST: Gas = 100;
    /// Per-byte extra cost of write access into storage.
    pub const WRITE_PER_BYTE_COST: Gas = 10;

    /* Rent-related constants */

    /// Cost of storing waitlisted message per block.
    pub const WAITLIST_COST: Gas = 100;
    /// Cost of storing reservation per block.
    pub const RESERVATION_COST: Gas = 100;
    /// Cost of storing delayed message per block.
    pub const DISPATCH_HOLD_COST: Gas = 100;
    /// Cost of storing program per block.
    ///
    /// (!) Currently disabled: storing programs are free.
    pub const RENT_COST: Value = 330;

    /* Execution-related constants */
    // TODO: use proper weights of instantiation and instrumentation (#3509).

    /// Maximal amount of reservations program may have.
    pub const MAX_RESERVATIONS: u64 = 256;
    /// Cost of wasm module instantiation before execution per byte of code.
    pub const MODULE_INSTANTIATION_BYTE_COST: Gas = 20;
    /// Cost of instrumenting wasm code on upload.
    pub const MODULE_INSTRUMENTATION_COST: Gas = 297;
    /// Cost of instrumenting wasm code on upload per byte of code.
    pub const MODULE_INSTRUMENTATION_BYTE_COST: Gas = 13;
    /// Initial random seed for testing environment.
    pub const INITIAL_RANDOM_SEED: u64 = 42;
}
