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

use crate::prelude::{Box, String, Vec};
use codec::Encode;
use scale_info::{MetaType, PortableRegistry, Registry};

pub fn to_wasm_ptr<T: AsRef<[u8]>>(bytes: T) -> *mut [i32; 2] {
    Box::into_raw(Box::new([
        bytes.as_ref().as_ptr() as _,
        bytes.as_ref().len() as _,
    ]))
}

pub fn to_hex_registry(meta_types: Vec<MetaType>) -> String {
    let mut registry = Registry::new();
    registry.register_types(meta_types);

    let registry: PortableRegistry = registry.into();
    hex::encode(registry.encode())
}

#[macro_export]
macro_rules! types {
    ($($t:ty), +) => { gstd::prelude::vec![$(scale_info::MetaType::new::<$t>()), +] };
}

#[macro_export]
macro_rules! declare {
    ($f:ident -> $txt:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $f() -> *mut [i32; 2] {
            gstd::meta::to_wasm_ptr($txt)
        }
    };
}

#[macro_export]
macro_rules! metadata {
    ($title:literal, $init_input:expr, $init_output:expr, $input:expr, $output:expr, $async_input:expr, $async_output:expr, $($t:ty), +) => {
        gstd::declare!(meta_title -> $title);
        gstd::declare!(meta_init_input -> $init_input);
        gstd::declare!(meta_init_output -> $init_output);
        gstd::declare!(meta_input -> $input);
        gstd::declare!(meta_output -> $output);
        gstd::declare!(meta_async_input-> $async_input);
        gstd::declare!(meta_async_output-> $async_output);
        gstd::declare!(meta_registry -> gstd::meta::to_hex_registry(gstd::types!($($t), +)));
    };
    // #1: all
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), stringify!($ho), stringify!($ai), stringify!($ao), $ii, $io, $hi, $ho, $ai, $ao);
    };
    // #2: no $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), stringify!($ho), stringify!($ai), "", $ii, $io, $hi, $ho, $ai);
    };
    // #3: no $ai
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), stringify!($ho), "", stringify!($ao), $ii, $io, $hi, $ho, $ao);
    };
    // #4: no $ai, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, output: $ho:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), stringify!($ho), "", "", $ii, $io, $hi, $ho);
    };
    // #5: no $ho
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), "", stringify!($ai), stringify!($ao), $ii, $io, $hi, $ai, $ao);
    };
    // #6: no $ho, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), "", stringify!($ai), "", $ii, $io, $hi, $ai);
    };
    // #7: no $ho, $ai
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), "", "", stringify!($ao), $ii, $io, $hi, $ao);
    };
    // #8: no $ho, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $hi:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($hi), "", "", "", $ii, $io, $hi);
    };
    // #9: no $hi
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($ho), stringify!($ai), stringify!($ao), $ii, $io, $ho, $ai, $ao);
    };
    // #10: no $hi, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($ho), stringify!($ai), "", $ii, $io, $ho, $ai);
    };
    // #11: no $hi, $ai
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($ho), "", stringify!($ao), $ii, $io, $ho, $ao);
    };
    // #12: no $hi, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $ho:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($ho), "", "", $ii, $io, $ho);
    };
    // #13: no $hi, $ho
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", stringify!($ai), stringify!($ao), $ii, $io, $ai, $ao);
    };
    // #14: no $hi, $ho, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", stringify!($ai), "", $ii, $io, $ai);
    };
    // #15: no $hi, $ho, $ai
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", "", stringify!($ao), $ii, $io, $ao);
    };
    // #16: no $hi, $ho, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, output: $io:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", "", "", $ii, $io);
    };
    // #17: no $io
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), stringify!($ho), stringify!($ai), stringify!($ao), $ii, $hi, $ho, $ai, $ao);
    };
    // #18: no $io, $ao
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), stringify!($ho), stringify!($ai), "", $ii, $hi, $ho, $ai);
    };
    // #19: no $io, $ai
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), stringify!($ho), "", stringify!($ao), $ii, $hi, $ho, $ao);
    };
    // #20: no $io, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, output: $ho:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), stringify!($ho), "", "", $ii, $hi, $ho);
    };
    // #21: no $io, $ho
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), "", stringify!($ai), stringify!($ao), $ii, $hi, $ai, $ao);
    };
    // #22: no $io, $ho, $ao
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), "", stringify!($ai), "", $ii, $hi, $ai);
    };
    // #23: no $io, $ho, $ai
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), "", "", stringify!($ao), $ii, $hi, $ao);
    };
    // #24: no $io, $ho, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, handle: input: $hi:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($hi), "", "", "", $ii, $hi);
    };
    // #25: no $io, $hi
    (title: $t:literal, init: input: $ii:ty, handle: output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($ho), stringify!($ai), stringify!($ao), $ii, $ho, $ai, $ao);
    };
    // #26: no $io, $hi, $ao
    (title: $t:literal, init: input: $ii:ty, handle: output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($ho), stringify!($ai), "", $ii, $ho, $ai);
    };
    // #27: no $io, $hi, $ai
    (title: $t:literal, init: input: $ii:ty, handle: output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($ho), "", stringify!($ao), $ii, $ho, $ao);
    };
    // #28: no $io, $hi, $ai, $ao
    (title: $t:literal, init: input: $ii:ty, handle: output: $ho:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($ho), "", "", $ii, $ho);
    };
    // #29: no $io, $hi, $ho
    (title: $t:literal, init: input: $ii:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", stringify!($ai), stringify!($ao), $ii, $ai, $ao);
    };
    // #30: no $io, $hi, $ho, $ao
    (title: $t:literal, init: input: $ii:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", stringify!($ai), "", $ii, $ai);
    };
    // #31: no $io, $hi, $ho, $ai
    (title: $t:literal, init: input: $ii:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", "", stringify!($ao), $ii, $ao);
    };
    // #32: no $io, $hi, $ho, $ai, $ao
    (title: $t:literal, init: input: $ii:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", "", "", $ii);
    };
    // #33: no $ii
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), stringify!($ho), stringify!($ai), stringify!($ao), $io, $hi, $ho, $ai, $ao);
    };
    // #34: no $ii, $ao
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), stringify!($ho), stringify!($ai), "", $io, $hi, $ho, $ai);
    };
    // #35: no $ii, $ai
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), stringify!($ho), "", stringify!($ao), $io, $hi, $ho, $ao);
    };
    // #36: no $ii, $ai, $ao
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, output: $ho:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), stringify!($ho), "", "", $io, $hi, $ho);
    };
    // #37: no $ii, $ho
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), "", stringify!($ai), stringify!($ao), $io, $hi, $ai, $ao);
    };
    // #38: no $ii, $ho, $ao
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), "", stringify!($ai), "", $io, $hi, $ai);
    };
    // #39: no $ii, $ho, $ai
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), "", "", stringify!($ao), $io, $hi, $ao);
    };
    // #40: no $ii, $ho, $ai, $ao
    (title: $t:literal, init: output: $io:ty, handle: input: $hi:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($hi), "", "", "", $io, $hi);
    };
    // #41: no $ii, $hi
    (title: $t:literal, init: output: $io:ty, handle: output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($ho), stringify!($ai), stringify!($ao), $io, $ho, $ai, $ao);
    };
    // #42: no $ii, $hi, $ao
    (title: $t:literal, init: output: $io:ty, handle: output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($ho), stringify!($ai), "", $io, $ho, $ai);
    };
    // #43: no $ii, $hi, $ai
    (title: $t:literal, init: output: $io:ty, handle: output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($ho), "", stringify!($ao), $io, $ho, $ao);
    };
    // #44: no $ii, $hi, $ai, $ao
    (title: $t:literal, init: output: $io:ty, handle: output: $ho:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($ho), "", "", $io, $ho);
    };
    // #45: no $ii, $hi, $ho
    (title: $t:literal, init: output: $io:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", stringify!($ai), stringify!($ao), $io, $ai, $ao);
    };
    // #46: no $ii, $hi, $ho, $ao
    (title: $t:literal, init: output: $io:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", stringify!($ai), "", $io, $ai);
    };
    // #47: no $ii, $hi, $ho, $ai
    (title: $t:literal, init: output: $io:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", "", stringify!($ao), $io, $ao);
    };
    // #48: no $ii, $hi, $ho, $ai, $ao
    (title: $t:literal, init: output: $io:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", "", "", $io);
    };
    // #49: no $ii, $io
    (title: $t:literal, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), stringify!($ho), stringify!($ai), stringify!($ao), $hi, $ho, $ai, $ao);
    };
    // #50: no $ii, $io, $ao
    (title: $t:literal, handle: input: $hi:ty, output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), stringify!($ho), stringify!($ai), "", $hi, $ho, $ai);
    };
    // #51: no $ii, $io, $ai
    (title: $t:literal, handle: input: $hi:ty, output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), stringify!($ho), "", stringify!($ao), $hi, $ho, $ao);
    };
    // #52: no $ii, $io, $ai, $ao
    (title: $t:literal, handle: input: $hi:ty, output: $ho:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), stringify!($ho), "", "", $hi, $ho);
    };
    // #53: no $ii, $io, $ho
    (title: $t:literal, handle: input: $hi:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), "", stringify!($ai), stringify!($ao), $hi, $ai, $ao);
    };
    // #54: no $ii, $io, $ho, $ao
    (title: $t:literal, handle: input: $hi:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), "", stringify!($ai), "", $hi, $ai);
    };
    // #55: no $ii, $io, $ho, $ai
    (title: $t:literal, handle: input: $hi:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), "", "", stringify!($ao), $hi, $ao);
    };
    // #56: no $ii, $io, $ho, $ai, $ao
    (title: $t:literal, handle: input: $hi:ty) => {
        gstd::metadata!($t, "", "", stringify!($hi), "", "", "", $hi);
    };
    // #57: no $ii, $io, $hi
    (title: $t:literal, handle: output: $ho:ty, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($ho), stringify!($ai), stringify!($ao), $ho, $ai, $ao);
    };
    // #58: no $ii, $io, $hi, $ao
    (title: $t:literal, handle: output: $ho:ty, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($ho), stringify!($ai), "", $ho, $ai);
    };
    // #59: no $ii, $io, $hi, $ai
    (title: $t:literal, handle: output: $ho:ty, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($ho), "", stringify!($ao), $ho, $ao);
    };
    // #60: no $ii, $io, $hi, $ai, $ao
    (title: $t:literal, handle: output: $ho:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($ho), "", "", $ho);
    };
    // #61: no $ii, $io, $hi, $ho
    (title: $t:literal, r#async: input: $ai:ty, output: $ao:ty) => {
        gstd::metadata!($t, "", "", "", "", stringify!($ai), stringify!($ao), $ai, $ao);
    };
    // #62: no $ii, $io, $hi, $ho, $ao
    (title: $t:literal, r#async: input: $ai:ty) => {
        gstd::metadata!($t, "", "", "", "", stringify!($ai), "", $ai);
    };
    // #63: no $ii, $io, $hi, $ho, $ai
    (title: $t:literal, r#async: output: $ao:ty) => {
        gstd::metadata!($t, "", "", "", "", "", stringify!($ao), $ao);
    };
}
