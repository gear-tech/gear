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

pub use paste::paste;

use crate::{static_mut, static_ref};
use arrayvec::ArrayVec;
use core::{mem, ptr};

static mut DTORS: ArrayVec<(Dtor, *mut ()), 32> = ArrayVec::new_const();

type Dtor = unsafe extern "C" fn(*mut ());
type AtExitFn = unsafe extern "C" fn();

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

#[macro_export]
macro_rules! dtor {
    (
        unsafe extern "C" fn $($priority:literal)?() {
            $($body:tt)*
        }
    ) => {
        $crate::ctor! {
            unsafe extern "C" fn $($priority)?() {
                unsafe extern "C" fn dtor() {
                    $($body)*
                }

                gcore::ctor::atexit(dtor);
            }
        }
    };
}

unsafe extern "C" {
    fn __gcore_set_fns(
        cxa_atexit: unsafe extern "C" fn(Dtor, *mut (), *mut ()) -> i32,
        dtors: unsafe extern "C" fn(),
    );

    pub fn atexit(func: AtExitFn) -> i32;
}

ctor! {
    unsafe extern "C" fn 10() {
        unsafe { __gcore_set_fns(cxa_atexit, dtors) };
        unsafe { static_mut!(DTORS).clear() };
    }
}

unsafe extern "C" fn cxa_atexit(func: Dtor, arg: *mut (), _dso: *mut ()) -> i32 {
    let dtors = unsafe { static_mut!(DTORS) };

    if let Err(_) = dtors.try_push((func, arg)) {
        return -1;
    }

    0
}

unsafe extern "C" fn dtors() {
    let dtors = unsafe { static_ref!(DTORS) };
    for &(dtor, arg) in dtors.iter().rev() {
        unsafe { dtor(arg) }
    }
}
