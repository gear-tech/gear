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

/// **The `declare!` macro**
#[macro_export]
macro_rules! declare {
    ($func_name:ident, $text:literal) => {
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> *mut [i32; 2] {
            crate::utils::return_slice($text.as_bytes())
        }
    };
    ($func_name:ident, $type:ty) => {
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> *mut [i32; 2] {
            let mut registry = Registry::new();
            let mut btree: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

            inspect!(btree, registry, $type);

            let json = serde_json::to_string(
                &serde_json::to_value(btree).unwrap()
            ).unwrap();

            crate::utils::return_slice(json.as_bytes())
        }
    };
    ($func_name:ident, $type:ty : $($others:ty), +) => {
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> *mut [i32; 2] {
            let mut registry = Registry::new();
            let mut btree: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

            inspect!(btree, registry, $type, $($others), +);

            let json = serde_json::to_string(
                &serde_json::to_value(btree).unwrap()
            ).unwrap();

            crate::utils::return_slice(json.as_bytes())
        }
    };
}
