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

#![deny(missing_docs)]
//! This crate provides a testing framework for testing gear programs.
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//!
//! use gstd::{msg, prelude::*};
//!
//! /// This program replies with `PONG` if it receives `PING`.
//! #[no_mangle]
//! extern "C" fn handle() {
//!     let payload = msg::load_bytes().expect("Failed to load payload");
//!
//!     if payload == b"PING" {
//!         msg::reply_bytes("PONG", 0).expect("Failed to send reply");
//!     }
//! }
//!
//! #[cfg(test)]
//! mod tests {
//!     use gtest::{Log, Program, System};
//!
//!     #[test]
//!     fn it_works() {
//!         let system = System::new();
//!         system.init_logger();
//!
//!         let from = 42;
//!         let program = Program::current(&system);
//!
//!         // Initialize program with message `()` from user `42`.
//!         {
//!             let res = program.send(from, ());
//!             let log = Log::builder().source(program.id()).dest(from);
//!             assert!(res.contains(&log));
//!         }
//!
//!         // Send message `b"PING"` to our program.
//!         {
//!             let res = program.send(from, *b"PING");
//!             let log = Log::builder()
//!                 .source(program.id())
//!                 .dest(from)
//!                 .payload(*b"PONG");
//!             assert!(res.contains(&log));
//!         }
//!     }
//! }
//! ```

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

/// Minimal amount of existence for account.
pub const EXISTENTIAL_DEPOSIT: u128 = 500;
/// Threshold for inserting into mailbox.
pub const MAILBOX_THRESHOLD: u64 = 3000;
/// Cost for single block waitlist holding.
pub const WAITLIST_COST: u64 = 100;
/// Reserve for parameter of scheduling.
pub const RESERVE_FOR: u32 = 1;
/// Cost for reservation holding.
pub const RESERVATION_COST: u64 = 100;
/// One-time db-read cost.
pub const READ_COST: u64 = 20;
/// One-time db-write cost.
pub const WRITE_COST: u64 = 100;
/// Per loaded byte cost.
pub const READ_PER_BYTE_COST: u64 = 10;
/// Per written byte cost.
pub const WRITE_PER_BYTE_COST: u64 = 10;
/// WASM module instantiation byte cost.
pub const MODULE_INSTANTIATION_BYTE_COST: u64 = 20;
/// Amount of reservations can exist for 1 program.
pub const MAX_RESERVATIONS: u64 = 256;
/// Epoch duration in blocks.
pub const EPOCH_DURATION_IN_BLOCKS: u32 = 600;
/// Initial random seed.
pub const INITIAL_RANDOM_SEED: u64 = 42;
/// WASM module instantiation byte cost.
pub const MODULE_INSTRUMENTATION_BYTE_COST: u64 = 13;
/// WASM module instantiation cost.
pub const MODULE_INSTRUMENTATION_COST: u64 = 297;
/// Cost of holding a message in dispatch stash.
pub const DISPATCH_HOLD_COST: u64 = 200;
/// Rent cost per block.
pub const RENT_COST: u128 = 330;
