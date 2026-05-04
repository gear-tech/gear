// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! Definition of a sandbox environment.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use parity_scale_codec::{Decode, Encode};
use sp_debug_derive::RuntimeDebug;
use sp_std::vec::Vec;
use sp_wasm_interface_common::ReturnValue;

#[derive(Clone, Copy, Debug)]
pub enum Instantiate {
    /// The first version of instantiate method and syscalls.
    Version1,
    /// The second version of syscalls changes their signatures to
    /// accept global gas value as its first argument and return the remaining
    /// gas value as its first result tuple element. The approach eliminates
    /// redundant host calls to get/set WASM-global value.
    Version2,
}

/// Error error that can be returned from host function.
#[derive(Encode, Decode, RuntimeDebug)]
pub struct HostError;

/// Describes an entity to define or import into the environment.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug)]
pub enum ExternEntity {
    /// Function that is specified by an index in a default table of
    /// a module that creates the sandbox.
    #[codec(index = 1)]
    Function(u32),

    /// Linear memory that is specified by some identifier returned by sandbox
    /// module upon creation new sandboxed memory.
    #[codec(index = 2)]
    Memory(u32),
}

/// An entry in a environment definition table.
///
/// Each entry has a two-level name and description of an entity
/// being defined.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug)]
pub struct Entry {
    /// Module name of which corresponding entity being defined.
    pub module_name: String,
    /// Field name in which corresponding entity being defined.
    pub field_name: String,
    /// External entity being defined.
    pub entity: ExternEntity,
}

/// Definition of runtime that could be used by sandboxed code.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug)]
pub struct EnvironmentDefinition {
    /// Vector of all entries in the environment definition.
    pub entries: Vec<Entry>,
}

/// Constant for specifying no limit when creating a sandboxed
/// memory instance. For FFI purposes.
pub const MEM_UNLIMITED: u32 = -1i32 as u32;

/// No error happened.
///
/// For FFI purposes.
pub const ERR_OK: u32 = 0;

/// Validation or instantiation error occurred when creating new
/// sandboxed module instance.
///
/// For FFI purposes.
pub const ERR_MODULE: u32 = -1i32 as u32;

/// Out-of-bounds access attempted with memory or table.
///
/// For FFI purposes.
pub const ERR_OUT_OF_BOUNDS: u32 = -2i32 as u32;

/// Execution error occurred (typically trap).
///
/// For FFI purposes.
pub const ERR_EXECUTION: u32 = -3i32 as u32;

/// A global variable has been successfully changed.
///
/// For FFI purposes.
pub const ERROR_GLOBALS_OK: u32 = 0;

/// A global variable is not found.
///
/// For FFI purposes.
pub const ERROR_GLOBALS_NOT_FOUND: u32 = u32::MAX;

/// A global variable is immutable or has a different type.
///
/// For FFI purposes.
pub const ERROR_GLOBALS_OTHER: u32 = u32::MAX - 1;

/// Typed value that can be returned from a wasm function
/// through the dispatch thunk.
/// Additionally contains globals values.
#[derive(Clone, Copy, PartialEq, Encode, Decode, Debug)]
pub struct WasmReturnValue {
    pub gas: i64,
    pub inner: ReturnValue,
}

impl WasmReturnValue {
    pub const ENCODED_MAX_SIZE: usize = 8 + ReturnValue::ENCODED_MAX_SIZE;
}

// TODO #3057
pub const GLOBAL_NAME_GAS: &str = "gear_gas";

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Codec;
    use std::fmt;

    fn roundtrip<S: Codec + PartialEq + fmt::Debug>(s: S) {
        let encoded = s.encode();
        assert_eq!(S::decode(&mut &encoded[..]).unwrap(), s);
    }

    #[test]
    fn env_def_roundtrip() {
        roundtrip(EnvironmentDefinition { entries: vec![] });

        roundtrip(EnvironmentDefinition {
            entries: vec![Entry {
                module_name: "kernel".to_string(),
                field_name: "memory".to_string(),
                entity: ExternEntity::Memory(1337),
            }],
        });

        roundtrip(EnvironmentDefinition {
            entries: vec![Entry {
                module_name: "env".to_string(),
                field_name: "abort".to_string(),
                entity: ExternEntity::Function(228),
            }],
        });
    }
}
