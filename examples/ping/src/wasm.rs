// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use gstd::{msg, vec};

use core::slice;

const SIZE: usize = 5_000_000;

#[no_mangle]
extern "C" fn handle() {
    let to = msg::source();
    let data = unsafe { slice::from_raw_parts(0x10usize as *mut u8, SIZE) };
    for _ in 0..150 {
        msg::send_bytes_delayed(to, data, 0, 1).unwrap();
    }
}

#[no_mangle]
extern "C" fn init() {
    let v = vec![1u8; SIZE];
    core::mem::forget(v);
}
