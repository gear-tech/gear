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

//! Stack allocations utils.

use core::mem::MaybeUninit;

/// Calls function `f` with provided byte buffer allocated on stack.
/// ### IMPORTANT
/// If buffer size is too big (currently bigger than 0x4000 bytes),
/// then allocation will be on heap.
/// If buffer is small enough to be allocated on stack, then real allocated
/// buffer size will be `size` aligned to upper power of 2.
pub fn with_byte_buffer<T>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> T) -> T {
    // TODO: consider to return error in case of heap allocation #2881
    gstack_buffer::with_byte_buffer(size, f)
}
