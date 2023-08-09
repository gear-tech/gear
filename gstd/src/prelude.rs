// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! The `gstd` default prelude. Re-imports default `std` modules and traits.
//! `std` can be safely replaced to `gstd` in the Rust programs.

// Reexports from Rust's libraries

pub use core::prelude::rust_2021::*;

pub use ::alloc::alloc;
pub use ::alloc::borrow;
pub use ::alloc::boxed;
pub use core::any;
pub use core::array;
pub use core::ascii;
pub use core::cell;
pub use core::char;
pub use core::clone;
pub use core::cmp;
/// Collection types.
///
/// See [`alloc::collections`] & [`hashbrown`].
///
/// [`alloc::collections`]: ::alloc::collections
pub mod collections {
    pub use ::alloc::collections::*;
    pub use ::hashbrown::{hash_map, hash_set, HashMap, HashSet};

    /// Reexports from [`hashbrown`].
    pub mod hashbrown {
        pub use hashbrown::{Equivalent, TryReserveError};
    }
}
pub use core::convert;
pub use core::default;
/// Utilities related to FFI bindings.
///
/// See [`alloc::ffi`] & [`core::ffi`].
///
/// [`alloc::ffi`]: ::alloc::ffi
pub mod ffi {
    pub use ::alloc::ffi::*;
    pub use core::ffi::*;
}
pub use ::alloc::fmt;
pub use ::alloc::rc;
pub use ::alloc::str;
pub use ::alloc::string;
pub use ::alloc::vec;
pub use core::future;
pub use core::hash;
pub use core::hint;
pub use core::iter;
pub use core::marker;
pub use core::mem;
pub use core::num;
pub use core::ops;
pub use core::option;
pub use core::panic;
pub use core::pin;
pub use core::primitive;
pub use core::ptr;
pub use core::result;
pub use core::slice;
pub use core::task;
pub use core::time;

pub use ::alloc::{
    borrow::ToOwned,
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
pub use core::{
    assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, matches, todo,
    unimplemented, unreachable, write, writeln,
};

// Reexports from third-party libraries

pub use parity_scale_codec::{self as codec, Decode, Encode};
pub use scale_info::{self, TypeInfo};
