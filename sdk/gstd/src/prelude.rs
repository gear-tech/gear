// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! The `gstd` default prelude. Re-imports default `std` modules and traits.
//! `std` can be safely replaced to `gstd` in the Rust programs.

// Reexports from Rust's libraries

#[cfg(not(feature = "ethexe"))]
pub use crate::ReservationIdExt;
pub use crate::{dbg, static_mut, static_ref};
pub use ::alloc::{
    borrow,
    borrow::ToOwned,
    boxed,
    boxed::Box,
    fmt, format, rc, str, string,
    string::{String, ToString},
    vec,
    vec::Vec,
};
pub use core::{
    any, array, ascii, assert_eq, assert_ne, cell, char, clone, cmp, convert, debug_assert,
    debug_assert_eq, debug_assert_ne, default, future, hash, hint, iter, marker, matches, mem, num,
    ops, option, panic, pin, prelude::rust_2021::*, primitive, ptr, result, slice, task, time,
    todo, unimplemented, unreachable, write, writeln,
};

/// Collection types.
///
/// See [`alloc::collections`] & [`hashbrown`].
///
/// [`alloc::collections`]: ::alloc::collections
pub mod collections {
    pub use ::alloc::collections::*;
    pub use ::hashbrown::{HashMap, HashSet, hash_map, hash_set};

    /// Reexports from [`hashbrown`].
    pub mod hashbrown {
        pub use hashbrown::{Equivalent, TryReserveError};
    }
}
/// Utilities related to FFI bindings.
///
/// See [`alloc::ffi`] & [`core::ffi`].
///
/// [`alloc::ffi`]: ::alloc::ffi
pub mod ffi {
    pub use ::alloc::ffi::{CString, FromVecWithNulError, IntoStringError, NulError};
    pub use core::ffi::*;
}

// Reexports from third-party libraries

pub use parity_scale_codec::{self as codec, Decode, Encode, EncodeLike, MaxEncodedLen};
pub use scale_info::{self, TypeInfo};
