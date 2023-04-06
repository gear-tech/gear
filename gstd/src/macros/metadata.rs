// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

/// Provide information about input and output types as the metadata.
///
/// Metadata can be used as a message payload description for external tools and applications that interact with Gear programs in the network. For example, in <https://idea.gear-tech.io>, it correctly forms a message payload from JSON on the JS application side.
///
/// This macro contains `input` and `output` message types for `init`, `handle`,
/// and `async` functions. It also contains the `state` output type used when
/// reading some part of the program's state.
///
/// # Examples
///
/// Define six custom types for input/output and provide information about them
/// as the metadata:
///
/// ```
/// use gstd::{metadata, msg, prelude::*};
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// struct InitInput {
///     field: String,
/// }
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// struct InitOutput {
///     field: String,
/// }
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// struct Input {
///     field: String,
/// }
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// struct Output {
///     field: String,
/// }
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// struct StateInput {
///     threshold: i32,
/// }
///
/// #[derive(Decode, Encode, TypeInfo)]
/// #[codec(crate = gstd::codec)]
/// enum StateOutput {
///     Small(i32),
///     Big(i32),
/// }
///
/// metadata! {
///     title: "App",
///     init:
///         input: InitInput,
///         output: InitOutput,
///     handle:
///         input: Input,
///         output: Output,
///     state:
///         input: StateInput,
///         output: StateOutput,
/// }
///
/// static mut STATE: i32 = 0;
///
/// #[no_mangle]
/// extern "C" fn init() {
///     let InitInput { field } = msg::load().expect("Unable to load");
///     let output = InitOutput { field };
///     msg::reply(output, 0).expect("Unable to reply");
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let Input { field } = msg::load().expect("Unable to load");
///     unsafe { STATE = 1000 };
///     let output = Output { field };
///     msg::reply(output, 0).expect("Unable to reply");
/// }
///
/// #[no_mangle]
/// extern "C" fn meta_state() -> *mut [i32; 2] {
///     let StateInput { threshold } = msg::load().expect("Unable to load");
///     let state = unsafe { STATE };
///     let result = if state > threshold {
///         StateOutput::Big(state)
///     } else {
///         StateOutput::Small(state)
///     };
///     gstd::util::to_leak_ptr(result.encode())
/// }
// ```
#[cfg(doc)]
#[macro_export]
macro_rules! metadata {
    ($arg:expr) => { ... };
}

#[cfg(not(doc))]
#[allow(missing_docs)]
#[deprecated(
    since = "0.1.0",
    note = "https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta"
)]
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
        $crate::export!(meta_title -> $title);
        $crate::export!(meta_init_input -> $init_input);
        $crate::export!(meta_init_output -> $init_output);
        $crate::export!(meta_async_init_input -> $async_init_input);
        $crate::export!(meta_async_init_output -> $async_init_output);
        $crate::export!(meta_handle_input -> $handle_input);
        $crate::export!(meta_handle_output -> $handle_output);
        $crate::export!(meta_async_handle_input -> $async_handle_input);
        $crate::export!(meta_async_handle_output -> $async_handle_output);
        $crate::export!(meta_state_input -> $state_input);
        $crate::export!(meta_state_output -> $state_output);
        $crate::export!(meta_registry -> $crate::util::to_hex_registry(
            $crate::prelude::vec![$($crate::util::MetaType::new::<$t>()), *]
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
        $crate::metadata!(
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
