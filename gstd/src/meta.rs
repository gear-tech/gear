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
    ($title:literal, $init_input:expr, $init_output:expr, $input:expr, $output:expr, $async_reply:expr, $($t:ty), +) => {
        gstd::declare!(meta_title -> $title);
        gstd::declare!(meta_init_input -> $init_input);
        gstd::declare!(meta_init_output -> $init_output);
        gstd::declare!(meta_input -> $input);
        gstd::declare!(meta_output -> $output);
        gstd::declare!(meta_async_reply -> $async_reply);
        gstd::declare!(meta_registry -> gstd::meta::to_hex_registry(gstd::types!($($t), +)));
    };
    // 1.1 all (with async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty, output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), stringify!($o), stringify!($r), $ii, $io, $i, $o, $r);
    };
    // 1.0 all (no async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), stringify!($o), "", $ii, $io, $i, $o);
    };
    // 2.1 no $o (with async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), "", stringify!($r), $ii, $io, $i, $r);
    };
    // 2.0 no $o (no async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), stringify!($i), "", "", $ii, $io, $i);
    };
    // 3.1 no $i (with async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($o), stringify!($r), $ii, $io, $o, $r);
    };
    // 3.0 no $i (no async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", stringify!($o), "", $ii, $io, $o);
    };
    // 4.1 no $i, $o (with async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", stringify!($r), $ii, $io, $r);
    };
    // 4.0 no $i, $o (no async reply)
    (title: $t:literal, init: input: $ii:ty, output: $io:ty) => {
        gstd::metadata!($t, stringify!($ii), stringify!($io), "", "", "", $ii, $io);
    };
    // 5.1 no $io (with async reply)
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty, output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), stringify!($o), stringify!($r), $ii, $i, $o, $r);
    };
    // 5.0 no $io (no async reply)
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), stringify!($o), "", $ii, $i, $o);
    };
    // 6.1 no $io, $o (with async reply)
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), "", stringify!($r), $ii, $i, $r);
    };
    // 6.0 no $io, $o (no async reply)
    (title: $t:literal, init: input: $ii:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, stringify!($ii), "", stringify!($i), "", "", $ii, $i);
    };
    // 7.1 no $io, $i (with async reply)
    (title: $t:literal, init: input: $ii:ty, handle: output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($o), stringify!($r), $ii, $o, $r);
    };
    // 7.0 no $io, $i (no async reply)
    (title: $t:literal, init: input: $ii:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", stringify!($o), "", $ii, $o);
    };
    // 8.1 no $io, $i, $o (with async reply)
    (title: $t:literal, init: input: $ii:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", stringify!($r), $ii, $r);
    };
    // 8.0 no $io, $i, $o (no async reply)
    (title: $t:literal, init: input: $ii:ty) => {
        gstd::metadata!($t, stringify!($ii), "", "", "", "", $ii);
    };
    // 9.1 no $ii (with async reply)
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty, output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), stringify!($o), stringify!($r), $io, $i, $o, $r);
    };
    // 9.0 no $ii (no async reply)
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), stringify!($o), "", $io, $i, $o);
    };
    // 10.1 no $ii, $o (with async reply)
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), "", stringify!($r), $io, $i, $r);
    };
    // 10.0 no $ii, $o (no async reply)
    (title: $t:literal, init: output: $io:ty, handle: input: $i:ty) => {
        gstd::metadata!($t, "", stringify!($io), stringify!($i), "", "", $io, $i);
    };
    // 11.1 no $ii, $i (with async reply)
    (title: $t:literal, init: output: $io:ty, handle: output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($o), stringify!($r), $io, $o, $r);
    };
    // 11.0 no $ii, $i (no async reply)
    (title: $t:literal, init: output: $io:ty, handle: output: $o:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", stringify!($o), "", $io, $o);
    };
    // 12.1 no $ii, $i, $o (with async reply)
    (title: $t:literal, init: output: $io:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", stringify!($r), $io, $r);
    };
    // 12.0 no $ii, $i, $o (no async reply)
    (title: $t:literal, init: output: $io:ty) => {
        gstd::metadata!($t, "", stringify!($io), "", "", "", $io);
    };
    // 13.1 no $ii, $io (with async reply)
    (title: $t:literal, handle: input: $i:ty, output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), stringify!($o), stringify!($r), $i, $o, $r);
    };
    // 13.0 no $ii, $io (no async reply)
    (title: $t:literal, handle: input: $i:ty, output: $o:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), stringify!($o), "", $i, $o);
    };
    // 14.1 no $ii, $io, $o (with async reply)
    (title: $t:literal, handle: input: $i:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), "", stringify!($r), $i, $r);
    };
    // 14.0 no $ii, $io, $o (no async reply)
    (title: $t:literal, handle: input: $i:ty) => {
        gstd::metadata!($t, "", "", stringify!($i), "", "", $i);
    };
    // 15.1 no $ii, $io, $i (with async reply)
    (title: $t:literal, handle: output: $o:ty, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($o), stringify!($r), $o, $r);
    };
    // 15.0 no $ii, $io, $i (no async reply)
    (title: $t:literal, handle: output: $o:ty) => {
        gstd::metadata!($t, "", "", "", stringify!($o), "", $o);
    };
    // 16 no $ii, $io, $i, $o (with async reply)
    (title: $t:literal, r#async: reply: $r:ty) => {
        gstd::metadata!($t, "", "", "", "", stringify!($r), $r);
    };
}
