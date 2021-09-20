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

use scale_info::{Registry, PortableRegistry, MetaType};
use crate::prelude::{Box, String, Vec};
use codec::Encode;

pub fn to_wasm_ptr<T: AsRef<[u8]>>(bytes: T) -> *mut [i32; 2] {
    Box::into_raw(Box::new([bytes.as_ref().as_ptr() as _, bytes.as_ref().len() as _]))
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
    }
}

#[macro_export]
macro_rules! metadata {
    (input: $title:literal, $init_input:expr, $init_output:expr, $input:expr, $output:expr, $($t:ty), +) => {
        gstd::declare!(meta_title -> $title);
        gstd::declare!(meta_init_input -> $init_input);
        gstd::declare!(meta_init_output -> $init_output);
        gstd::declare!(meta_input -> $input);
        gstd::declare!(meta_output -> $output);
        gstd::declare!(registry -> crate::meta::to_hex_registry(crate::types!($($t), +)));
    };
    // 1 all
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), stringify!($o), $ii, $io, $i, $o);
    };
    // 2 no $o
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), "", $ii, $io, $i);
    };
    // 3 no $i
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($o), $ii, $io, $o);
    };
    // 4 no $i, $o
    (title: $t:literal, init: input: $ii:ty, output: $io:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", $ii, $io);
    };
    // 5 no $io
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), stringify!($o), $ii, $i, $o);
    };
    // 6 no $io, $o
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), "", $ii, $i);
    };
    // 7 no $io, $i
    (title: $t:literal, init: input: $ii:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($o), $ii, $o);
    };
    // 8 no $io, $i, $o
    (title: $t:literal, init: input: $ii:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", $ii);
    };
    // 9 no $ii
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), stringify!($o), $io, $i, $o);
    };
    // 10 no $ii, $o
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), "", $io, $i);
    };
    // 11 no $ii, $i
    (title: $t:literal, init: output: $io:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($o), $io, $o);
    };
    // 12 no $ii, $i, $o
    (title: $t:literal, init: output: $io:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", $io);
    };
    // 13 no $ii, $io
    (title: $t:literal, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), stringify!($o), $i, $o);
    };
    // 14 no $ii, $io, $o
    (title: $t:literal, handle: input: $i:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), "", $i);
    };
    // 15 no $ii, $io, $i
    (title: $t:literal, handle: output: $o:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($o), $o);
    };
}
