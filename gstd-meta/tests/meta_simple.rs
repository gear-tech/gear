// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![no_std]

use gstd_meta::*;

extern crate alloc;
use alloc::{
    boxed::Box,
    string::{String, ToString},
};

#[derive(TypeInfo)]
pub struct SaltAmount {
    pub value: u8,
}

#[derive(TypeInfo)]
pub struct Egg {
    pub weight: u32,
    pub ostrich: bool,
}

// Function for more visual testing
fn compare_len<T: ToString>(raw: *mut [i32; 2], expected: T) {
    assert_eq!(
        unsafe { Box::from_raw(raw) }[1] as usize,
        expected.to_string().len()
    )
}

meta! {
    title: "Test title level simple",
    input: SaltAmount,
    output: String,
    init_input: Egg,
    init_output: u8
}

#[test]
fn find_meta_without_extra_types() {
    compare_len(unsafe { meta_title() }, "Test title level simple");

    compare_len(unsafe { meta_input() }, "SaltAmount");

    compare_len(unsafe { meta_output() }, "String");

    compare_len(unsafe { meta_init_input() }, "Egg");

    compare_len(unsafe { meta_init_output() }, "u8");

    compare_len(
        unsafe { meta_types() },
        r#"{
            "Egg": {
              "ostrich": "bool",
              "weight": "u32"
            },
            "SaltAmount": {
              "value": "u8"
            }
          }"#
        .replace("\n", "")
        .replace(" ", ""),
    );
}
