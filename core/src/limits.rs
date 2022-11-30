// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Limits for various metrics.

use crate::code;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// Describes the upper limits on various metrics.
///
/// # Note
///
/// The values in this struct should never be decreased. The reason is that decreasing those
/// values will break existing programs which are above the new limits when a
/// re-instrumentation is triggered.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize)]
pub struct Limits {
    /// Maximum allowed stack height in number of elements.
    ///
    /// See <https://wiki.parity.io/WebAssembly-StackHeight> to find out
    /// how the stack frame cost is calculated. Each element can be of one of the
    /// wasm value types. This means the maximum size per element is 64bit.
    ///
    /// # Note
    ///
    /// It is safe to disable (pass `None`) the `stack_height` when the execution engine
    /// is part of the runtime and hence there can be no indeterminism between different
    /// client resident execution engines.
    pub stack_height: Option<u32>,

    /// Maximum number of globals a module is allowed to declare.
    ///
    /// Globals are not limited through the `stack_height` as locals are. Neither does
    /// the linear memory limit `memory_pages` applies to them.
    pub globals: u32,

    /// Maximum numbers of parameters a function can have.
    ///
    /// Those need to be limited to prevent a potentially exploitable interaction with
    /// the stack height instrumentation: The costs of executing the stack height
    /// instrumentation for an indirectly called function scales linearly with the amount
    /// of parameters of this function. Because the stack height instrumentation itself is
    /// is not weight metered its costs must be static (via this limit) and included in
    /// the costs of the instructions that cause them (call, call_indirect).
    pub parameters: u32,

    /// Maximum number of memory pages allowed for a program.
    pub memory_pages: u32,

    /// Maximum number of elements allowed in a table.
    ///
    /// Currently, the only type of element that is allowed in a table is funcref.
    pub table_size: u32,

    /// Maximum number of elements that can appear as immediate value to the br_table instruction.
    pub br_table_size: u32,

    /// The maximum length of a subject in bytes used for PRNG generation.
    pub subject_len: u32,

    /// The maximum nesting level of the call stack.
    pub call_depth: u32,

    /// The maximum size of a message payload in bytes.
    pub payload_len: u32,

    /// The maximum length of a program code in bytes. This limit applies to the instrumented
    /// version of the code. Therefore `instantiate_with_code` can fail even when supplying
    /// a wasm binary below this maximum size.
    pub code_len: u32,
}

impl Limits {
    /// The maximum memory size in bytes that a program can occupy.
    pub fn max_memory_size(&self) -> u32 {
        self.memory_pages * 64 * 1024
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stack_height: None,
            globals: 256,
            parameters: 128,
            memory_pages: code::MAX_WASM_PAGE_COUNT,
            // 4k function pointers (This is in count not bytes).
            table_size: 4096,
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: 16 * 64 * 1024,
            code_len: 512 * 1024,
        }
    }
}
