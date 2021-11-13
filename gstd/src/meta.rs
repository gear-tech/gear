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
    ($($t:ty), *) => { gstd::prelude::vec![$(scale_info::MetaType::new::<$t>()), *] };
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
    (
        $title:literal,
        $init_input:expr,
        $init_output:expr,
        $async_init_input:expr,
        $async_init_output:expr,
        $handle_input:expr,
        $handle_output:expr,
        $async_handle_input:expr,
        $async_handle_output:expr,
        $state_input:expr,
        $state_output:expr
        $(, $t:ty) *) => {
        gstd::declare!(meta_title -> $title);
        gstd::declare!(meta_init_input -> $init_input);
        gstd::declare!(meta_init_output -> $init_output);
        gstd::declare!(meta_async_init_input -> $async_init_input);
        gstd::declare!(meta_async_init_output -> $async_init_output);
        gstd::declare!(meta_handle_input -> $handle_input);
        gstd::declare!(meta_handle_output -> $handle_output);
        gstd::declare!(meta_async_handle_input -> $async_handle_input);
        gstd::declare!(meta_async_handle_output -> $async_handle_output);
        gstd::declare!(meta_state_input -> $state_input);
        gstd::declare!(meta_state_output -> $state_output);
        gstd::declare!(meta_registry -> gstd::meta::to_hex_registry(gstd::types!($($t), *)));
    };

    (
        title: $title:literal, // program title
        $(
            init: // init messaging types
                $(input: $ii:ty,)? // init input
                $(output: $io:ty,)? // init output
            $(
                awaiting:
                    $(input: $aii:ty,)? // async init input
                    $(output: $aio:ty,)? // async init output
            )?
        )?
        $(
            handle: // handle messaging types
                $(input: $hi:ty,)?
                $(output: $ho:ty,)?
            $(
                awaiting:
                    $(input: $ahi:ty,)? // async handle input
                    $(output: $aho:ty,)? // async handle output
            )?
        )?
        $(
            state:
                $(input: $si:ty,)?
                $(output: $so:ty,)?
        )?
    ) => {
        gstd::metadata!(
            $title, // program title
            stringify!($($($ii)?)?), // init input
            stringify!($($($io)?)?), // init output
            stringify!($($($($aii)?)?)?), // async init input
            stringify!($($($($aio)?)?)?), // async init output
            stringify!($($($hi)?)?), // handle input
            stringify!($($($ho)?)?), // handle output
            stringify!($($($($ahi)?)?)?), // async handle input
            stringify!($($($($aho)?)?)?), // async handle output
            stringify!($($($si)?)?), // state input
            stringify!($($($so)?)?) // state output
            $($(, $ii)?)? // init input
            $($(, $io)?)? // init output
            $($($(, $aii)?)?)? // async init input
            $($($(, $aio)?)?)? // async init output
            $($(, $hi)?)? // handle input
            $($(, $ho)?)?// handle output
            $($($(, $ahi)?)?)? // async handle input
            $($($(, $aho)?)?)? // async handle output
            $($(, $si)?)? // state input
            $($(, $so)?)? // state output
        );
    };
}
