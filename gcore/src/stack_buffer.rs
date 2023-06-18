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

//! +_+_+

use alloc::vec;

#[inline(never)]
fn with_byte_array<T, const N: usize>(size: usize, f: impl FnOnce(&mut [u8]) -> T) -> T {
    let mut buffer = [0u8; N];
    let sub_buffer = &mut buffer[0..size];
    f(sub_buffer)
}

/// +_+_+
pub fn with_byte_buffer<T>(size: usize, f: impl FnOnce(&mut [u8]) -> T) -> T {
    match size {
        size if size <= 0x1 => with_byte_array::<_, 0x1>(size, f),
        size if size <= 0x2 => with_byte_array::<_, 0x2>(size, f),
        size if size <= 0x4 => with_byte_array::<_, 0x4>(size, f),
        size if size <= 0x8 => with_byte_array::<_, 0x8>(size, f),
        size if size <= 0x10 => with_byte_array::<_, 0x10>(size, f),
        size if size <= 0x20 => with_byte_array::<_, 0x20>(size, f),
        size if size <= 0x40 => with_byte_array::<_, 0x40>(size, f),
        size if size <= 0x80 => with_byte_array::<_, 0x80>(size, f),
        size if size <= 0x100 => with_byte_array::<_, 0x100>(size, f),
        size if size <= 0x200 => with_byte_array::<_, 0x200>(size, f),
        size if size <= 0x400 => with_byte_array::<_, 0x400>(size, f),
        size if size <= 0x800 => with_byte_array::<_, 0x800>(size, f),
        size if size <= 0x1000 => with_byte_array::<_, 0x1000>(size, f),
        size if size <= 0x2000 => with_byte_array::<_, 0x2000>(size, f),
        size if size <= 0x4000 => with_byte_array::<_, 0x4000>(size, f),
        _ => f(vec![0; size].as_mut_slice()),
    }
}
