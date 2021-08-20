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

#[gear_data]
struct Abc {
    _a: u8,
    _b: u16,
}

#[test]
fn check() {
    let mut registry = Registry::new();
    let mut btree: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    inspect!(btree, registry, Abc);

    let string = serde_json::to_string(&serde_json::to_value(btree).unwrap()).unwrap();
    assert_eq!(string, r#"{"Abc":{"_a":"u8","_b":"u16"}}"#);
}
