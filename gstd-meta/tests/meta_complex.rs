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
use alloc::{boxed::Box, string::String, vec::Vec};

#[derive(TypeInfo)]
pub struct SaltAmount {
    pub value: u8,
}

#[derive(TypeInfo)]
pub struct Meat {
    pub name: String,
    pub salt: Option<SaltAmount>,
}

#[derive(TypeInfo)]
pub struct Egg {
    pub weight: u32,
    pub ostrich: bool,
}

#[derive(TypeInfo)]
pub struct Sauce {
    pub eggs: Vec<Egg>,
    pub salty: Result<SaltAmount, SaltAmount>,
}

#[derive(TypeInfo)]
pub struct Meal {
    pub steak: Meat,
    pub mayonnaise: Sauce,
}

// Function for more visual testing
fn compare_len(raw: *mut [i32; 2], expected: &'static str) {
    assert_eq!(
        unsafe { Box::from_raw(raw) }[1] as usize,
        expected.replace("\n", "").replace(" ", "").len()
    )
}

// Function for more visual testing
fn compare_title_len(raw: *mut [i32; 2], expected: &'static str) {
    assert_eq!(unsafe { Box::from_raw(raw) }[1] as usize, expected.len())
}

meta! {
    title: "Test title level complex",
    input: Meal,
    output: Meat,
    init_input: Egg,
    init_output: Sauce,
    extra: Meat, Sauce, Egg, SaltAmount
}

#[test]
fn find_meta_with_extra_types() {
    compare_title_len(unsafe { meta_title() }, r#"Test title level complex"#);

    compare_len(
        unsafe { meta_input() },
        r#"[
            {
              "Meal": {
                "mayonnaise": "Sauce",
                "steak": "Meat"
              }
            },
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "Sauce": {
                "eggs": "Vec<Egg>",
                "salty": "Result<SaltAmount,SaltAmount>"
              }
            },
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );

    compare_len(
        unsafe { meta_output() },
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );

    compare_len(
        unsafe { meta_init_input() },
        r#"[
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            }
        ]"#,
    );

    compare_len(
        unsafe { meta_init_output() },
        r#"[
            {
              "Sauce": {
                "eggs": "Vec<Egg>",
                "salty": "Result<SaltAmount,SaltAmount>"
              }
            },
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );
}
