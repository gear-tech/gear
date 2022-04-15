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

use crate::{GAS_ALLOWANCE_STR, LEAVE_TRAP_STR, WAIT_TRAP_STR};
use alloc::{vec, vec::Vec};
use gear_core::memory::Memory;

pub fn is_wait_trap(trap: &str) -> bool {
    trap.starts_with(WAIT_TRAP_STR)
}

pub fn is_leave_trap(trap: &str) -> bool {
    trap.starts_with(LEAVE_TRAP_STR)
}

pub fn is_gas_allowance_trap(trap: &str) -> bool {
    trap.starts_with(GAS_ALLOWANCE_STR)
}

pub fn get_bytes32(mem: &dyn Memory, ptr: usize) -> Result<[u8; 32], &'static str> {
    let mut ret = [0u8; 32];
    mem.read(ptr, &mut ret)
        .map_err(|_| "Cannot read 32 bytes from mem")?;
    Ok(ret)
}

pub fn get_u128(mem: &dyn Memory, ptr: usize) -> Result<u128, &'static str> {
    let mut u128_le = [0u8; 16];
    mem.read(ptr, &mut u128_le)
        .map_err(|_| "Cannot read 16 bytes from mem")?;
    Ok(u128::from_le_bytes(u128_le))
}

pub fn get_vec(mem: &dyn Memory, ptr: usize, len: usize) -> Result<Vec<u8>, &'static str> {
    let mut vec = vec![0u8; len];
    mem.read(ptr, &mut vec)
        .map_err(|_| "Cannot read bytes from mem")?;
    Ok(vec)
}

pub fn set_u128(mem: &mut dyn Memory, ptr: usize, val: u128) -> Result<(), &'static str> {
    mem.write(ptr, &val.to_le_bytes())
        .map_err(|_| "Cannot set u128 in memory")
}
