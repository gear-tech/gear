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

//! Gear `metadata!` macro. Exports functions with IO data.

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
        gstd::export!(meta_title -> $title);
        gstd::export!(meta_init_input -> $init_input);
        gstd::export!(meta_init_output -> $init_output);
        gstd::export!(meta_async_init_input -> $async_init_input);
        gstd::export!(meta_async_init_output -> $async_init_output);
        gstd::export!(meta_handle_input -> $handle_input);
        gstd::export!(meta_handle_output -> $handle_output);
        gstd::export!(meta_async_handle_input -> $async_handle_input);
        gstd::export!(meta_async_handle_output -> $async_handle_output);
        gstd::export!(meta_state_input -> $state_input);
        gstd::export!(meta_state_output -> $state_output);
        gstd::export!(meta_registry -> gstd::macros::util::to_hex_registry(
            gstd::prelude::vec![$(gstd::macros::util::MetaType::new::<$t>()), *]
        ));
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
