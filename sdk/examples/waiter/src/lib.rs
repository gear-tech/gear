// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
    gstd::SYSTEM_RESERVE
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
