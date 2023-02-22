// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

pub use error::{Result, TestError};
pub use log::{CoreLog, Log, RunResult};
pub use program::{calculate_program_id, Gas, Program, WasmProgram};
pub use system::System;

pub const EXISTENTIAL_DEPOSIT: u128 = 500;
pub const MAILBOX_THRESHOLD: u64 = 3000;
pub const WAITLIST_COST: u64 = 100;
pub const RESERVE_FOR: u32 = 1;
pub const RESERVATION_COST: u64 = 100;
pub const READ_COST: u64 = 20;
pub const WRITE_COST: u64 = 100;
pub const READ_PER_BYTE_COST: u64 = 10;
pub const WRITE_PER_BYTE_COST: u64 = 10;
pub const MODULE_INSTANTIATION_BYTE_COST: u64 = 20;
pub const MAX_RESERVATIONS: u64 = 256;
pub const EPOCH_DURATION_IN_BLOCKS: u32 = 600;
pub const INITIAL_RANDOM_SEED: u64 = 42;
pub const MODULE_INSTRUMENTATION_BYTE_COST: u64 = 13;
pub const MODULE_INSTRUMENTATION_COST: u64 = 297;
pub const DISPATCH_HOLD_COST: u64 = 200;
