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
#![no_std]

use codec::{Decode, Encode};
use gstd::{ActorId, Vec};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    include! {"./code.rs"}
}

pub fn system_reserve() -> u64 {
    gstd::Config::system_reserve()
}

// Re-exports for testing
pub fn default_wait_up_to_duration() -> u32 {
    gstd::Config::wait_up_to()
}

#[derive(Debug, Encode, Decode)]
pub enum WaitSubcommand {
    Wait,
    WaitFor(u32),
    WaitUpTo(u32),
}

#[derive(Debug, Encode, Decode)]
pub enum SleepForWaitType {
    All,
    Any,
}

#[derive(Debug, Encode, Decode)]
pub enum Command {
    Wait(WaitSubcommand),
    SendFor(ActorId, u32),
    SendUpTo(ActorId, u32),
    SendUpToWait(ActorId, u32),
    SendAndWaitFor(u32, ActorId),
    ReplyAndWait(WaitSubcommand),
    SleepFor(Vec<u32>, SleepForWaitType),
    WakeUp([u8; 32]),
}
