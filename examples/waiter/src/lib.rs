// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use gcore::BlockCount;
use parity_scale_codec::{Decode, Encode};

type ActorId = [u8; 32];

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;

#[cfg(feature = "std")]
pub fn system_reserve() -> u64 {
    gstd::Config::system_reserve()
}

// Re-exports for testing
#[cfg(feature = "std")]
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
pub enum LockContinuation {
    Nothing,
    SleepFor(u32),
    MoveToStatic,
    Wait,
    Forget,
}

#[derive(Debug, Encode, Decode)]
pub enum MxLockContinuation {
    Lock,
    General(LockContinuation),
}

#[derive(Debug, Encode, Decode)]
pub enum LockStaticAccessSubcommand {
    Drop,
    AsRef,
    AsMut,
    Deref,
    DerefMut,
}

#[derive(Debug, Encode, Decode, Clone, Copy)]
pub enum RwLockType {
    Read,
    Write,
}

#[derive(Debug, Encode, Decode)]
pub enum RwLockContinuation {
    // Here will be Lock(RwLockType)
    General(LockContinuation),
}

#[derive(Debug, Encode, Decode)]
pub enum Command {
    Wait(WaitSubcommand),
    SendFor(ActorId, BlockCount),
    SendUpTo(ActorId, BlockCount),
    SendUpToWait(ActorId, BlockCount),
    SendAndWaitFor(BlockCount, ActorId),
    ReplyAndWait(WaitSubcommand),
    SleepFor(Vec<BlockCount>, SleepForWaitType),
    WakeUp([u8; 32]),
    MxLock(Option<BlockCount>, MxLockContinuation),
    MxLockStaticAccess(LockStaticAccessSubcommand),
    RwLock(RwLockType, RwLockContinuation),
    RwLockStaticAccess(RwLockType, LockStaticAccessSubcommand),
}
