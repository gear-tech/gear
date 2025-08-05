// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use std::mem;

pub fn string_to_hex(hex: &str) -> [u8; 32] {
    // Convert hex string to bytes
    let bytes = hex::decode(hex).expect("Invalid hex string");
    // Ensure the length is 32 bytes
    if bytes.len() != 32 {
        panic!("Hex string must be 32 bytes long");
    }
    // Convert bytes to array
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);
    array
}

pub fn hex_to_string(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

// Convert a vector of u32 to a vector of u8
pub fn cast_vec(mut input: Vec<u32>) -> Vec<u8> {
    let ptr = input.as_mut_ptr();
    let length = input.len();
    let capacity = input.capacity();
    let _ = input.leak(); // Prevent Rust from freeing the memory
    unsafe {
        Vec::from_raw_parts(
            ptr as *mut u8,
            length * mem::size_of::<u32>(),
            capacity * mem::size_of::<u32>(),
        )
    }
}

pub fn cast_slice_mut(input: &mut [u32]) -> &mut [u8] {
    let ptr = input.as_mut_ptr();
    unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, mem::size_of_val(input)) }
}

pub fn cast_slice(input: &[u32]) -> &[u8] {
    let ptr = input.as_ptr();
    unsafe { std::slice::from_raw_parts(ptr as *const u8, mem::size_of_val(input)) }
}

#[allow(dead_code)]
pub fn simulate_panic(b: &[u8]) {
    if b[0] % 100 == 32 && b[1] % 100 == 42 {
        eprintln!("{b:X?}");
        eprintln!("pid: {}", std::process::id());
        panic!("Simulated panic in worker process");
    }
}
