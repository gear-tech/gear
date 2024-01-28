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

//! TODO.

use std::mem;

/// Represents memory layout that can be safely used between syscalls and instructions.
pub struct MemoryLayout {
    pub init_called_ptr: i32,
    pub wait_called_ptr: i32,
    pub remaining_memory_len: u32,
    pub remaining_memory_ptr: i32,
}

impl MemoryLayout {
    /// The amount of reserved memory.
    pub const RESERVED_MEMORY_SIZE: u32 = 256;
}

impl From<u32> for MemoryLayout {
    fn from(mem_size: u32) -> Self {
        let start_memory_ptr = mem_size.saturating_sub(Self::RESERVED_MEMORY_SIZE) as i32;
        let init_called_ptr = start_memory_ptr;
        let wait_called_ptr = init_called_ptr + mem::size_of::<bool>() as i32;
        let remaining_memory_ptr = wait_called_ptr + mem::size_of::<u32>() as i32;
        let remaining_memory_len = (remaining_memory_ptr - start_memory_ptr) as u32;

        assert!(
            remaining_memory_len <= Self::RESERVED_MEMORY_SIZE,
            "reserved memory exceeded"
        );

        Self {
            init_called_ptr,
            wait_called_ptr,
            remaining_memory_len,
            remaining_memory_ptr,
        }
    }
}
