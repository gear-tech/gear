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

use alloc::{vec, vec::Vec};
use gear_core::memory::Memory;
use gear_core_errors::MemoryError;

pub fn get_bytes32(mem: &impl Memory, ptr: usize) -> Result<[u8; 32], MemoryError> {
    let mut ret = [0u8; 32];
    mem.read(ptr, &mut ret)?;
    Ok(ret)
}

pub fn get_u128(mem: &impl Memory, ptr: usize) -> Result<u128, MemoryError> {
    let mut u128_le = [0u8; 16];
    mem.read(ptr, &mut u128_le)?;
    Ok(u128::from_le_bytes(u128_le))
}

pub fn get_vec(mem: &impl Memory, ptr: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
    let mut vec = vec![0u8; len];
    mem.read(ptr, &mut vec)?;
    Ok(vec)
}

pub fn set_u128(mem: &mut impl Memory, ptr: usize, val: u128) -> Result<(), MemoryError> {
    mem.write(ptr, &val.to_le_bytes())
}
