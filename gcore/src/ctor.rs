// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Global constructors and destructors.

use crate::{static_mut, static_ref};
use arrayvec::ArrayVec;
use core::ffi::c_void;
use core::{mem, ptr};

#[doc(hidden)]
pub use paste::paste;

static mut DTORS: ArrayVec<(Dtor, *mut c_void), 32> = ArrayVec::new_const();

// a symbol that forces the linker to retain `.init_array` entries
// so that the linker does not garbage-collect constructors
#[unsafe(no_mangle)]
#[used]
#[allow(non_upper_case_globals)]
static __gcore_pull_in_symbol: u8 = 0;

type Dtor = unsafe extern "C" fn(*mut c_void);

/// Defines a global constructor.
///
/// The function is executed at the start of **every entrypoint** invocation.
///
/// # Examples
///
/// ```rust,no_run
/// gcore::ctor! {
///     unsafe extern "C" fn() {
///         // your global constructor
///     }
/// }
/// ```
///
/// # Priority
///
/// Constructors are ordered by a numeric priority: **the lower the priority,
/// the earlier it runs**.
///
/// Priorities `0..=999` are reserved for internal/runtime usage.
///
/// ```rust,no_run
/// // runs first
/// gcore::ctor! {
///     unsafe extern "C" fn 50000() {
///         // your global constructor
///     }
/// }
///
/// // runs second
/// gcore::ctor! {
///     unsafe extern "C" fn 50001() {
///         // your global constructor
///     }
/// }
/// ```
#[macro_export]
macro_rules! ctor {
    (
        unsafe extern "C" fn $priority:literal() {
            $($body:tt)*
        }
    ) => {
        const _: () = {
            $crate::ctor::paste! {
                #[unsafe(link_section = ".init_array." $priority )]
                #[used]
                static _FUNC: unsafe extern "C" fn() = {
                    unsafe extern "C" fn ctor() {
                        $($body)*
                    }

                    ctor
                };
            }
        };
    };
    (
        unsafe extern "C" fn() {
            $($body:tt)*
        }
    ) => {
        $crate::ctor! {
            unsafe extern "C" fn 65535() {
                $($body)*
            }
        }
    };
}

/// Defines a global destructor.
///
/// The function is executed at the **end of every entrypoint** invocation.
///
/// This is a thin wrapper around [`ctor!`] that registers the body via
/// [`atexit()`], so the same priority rules and limits apply.
///
/// **Note:** because the wrapper always calls [`atexit()`], the destructor
/// is always registered for any entrypoint.
///
/// # Examples
///
/// ```rust,no_run
/// gcore::dtor! {
///     unsafe extern "C" fn() {
///         // your global destructor
///     }
/// }
/// ```
///
/// # Priority and ordering
///
/// See the [priority](ctor#priority) docs on [`ctor!`]. Because destructors
/// are executed in reverse registration order (LIFO), a destructor associated
/// with a **higher** constructor priority (runs later at start) will run
/// **earlier** at shutdown.
///
/// ```rust,no_run
/// // registered earlier → runs second (later) at shutdown
/// gcore::dtor! {
///     unsafe extern "C" fn 50000() {
///         // your global destructor
///     }
/// }
///
/// // registered later → runs first (earlier) at shutdown
/// gcore::dtor! {
///     unsafe extern "C" fn 50001() {
///         // your global destructor
///     }
/// }
/// ```
#[macro_export]
macro_rules! dtor {
    (
        unsafe extern "C" fn $($priority:literal)?() {
            $($body:tt)*
        }
    ) => {
        $crate::ctor! {
            unsafe extern "C" fn $($priority)?() {
                $crate::ctor::atexit(|| {
                    $($body)*
                });
            }
        }
    };
}

/// Registers a function to be executed at entry point termination.
///
/// Returns `0` on success, or `-1` if the registration limit has been reached.
///
/// Registered functions:
/// - are executed in **reverse order of registration** (LIFO) at the end of the
///   current entry point;
/// - remain registered until that point (they are not automatically
///   unregistered before then);
/// - are limited to a maximum of **32** slots total. Core libraries such as
///   `gcore`, `gstd`, and `galloc` may consume some of these slots, reducing
///   what is available to user code.
///
/// # Examples
///
/// ```rust,no_run
/// fn cleanup() {
///     /* ... */
/// }
///
/// let rc = gcore::atexit(cleanup);
/// assert_eq!(rc, 0, "atexit registry is full");
/// ```
pub fn atexit(func: fn()) -> i32 {
    unsafe extern "C" fn call(arg: *mut c_void) {
        let func = unsafe { mem::transmute::<*mut c_void, fn()>(arg) };
        func()
    }

    unsafe { __cxa_atexit_impl(call, func as *mut c_void, ptr::null_mut()) }
}

unsafe extern "C" {
    fn __gcore_set_fns(
        __cxa_atexit_impl: unsafe extern "C" fn(Dtor, *mut c_void, *mut c_void) -> i32,
        dtors: unsafe extern "C" fn(),
    );
}

ctor! {
    unsafe extern "C" fn 10() {
        unsafe { __gcore_set_fns(__cxa_atexit_impl, dtors) };
        unsafe { static_mut!(DTORS).clear() };
    }
}

#[doc(hidden)]
pub unsafe extern "C" fn __cxa_atexit_impl(func: Dtor, arg: *mut c_void, _dso: *mut c_void) -> i32 {
    let dtors = unsafe { static_mut!(DTORS) };

    if dtors.try_push((func, arg)).is_err() {
        return -1;
    }

    0
}

pub(crate) unsafe extern "C" fn dtors() {
    let dtors = unsafe { static_ref!(DTORS) };
    for &(dtor, arg) in dtors.iter().rev() {
        unsafe { dtor(arg) }
    }
}
