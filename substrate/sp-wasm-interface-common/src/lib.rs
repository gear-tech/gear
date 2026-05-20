// This file is part of Gear.
//
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear-local compatibility surface for wasm interface helpers that were previously carried in
//! the Polkadot SDK fork.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod util;

pub use sp_wasm_interface::{
    IntoValue, Pointer, PointerType, ReturnValue, Signature, TryFromValue, Value, ValueType,
    WordSize,
};

/// Identifier of a sandbox memory allocated by the runtime interface.
pub type MemoryId = u32;

/// Raw host pointer large enough to carry native pointers through the runtime interface.
pub type HostPointer = u64;
