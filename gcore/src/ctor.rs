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

use crate::{static_mut, static_ref};
use arrayvec::ArrayVec;
use core::{mem, ptr};

static mut DTORS: ArrayVec<(Dtor, *mut ()), 32> = ArrayVec::new_const();

type Dtor = unsafe extern "C" fn(*mut ());
type AtExitFn = unsafe extern "C" fn();

#[macro_export]
macro_rules! ctor {
    (
        unsafe extern "C" fn() {
            $($body:tt)*
        }
    ) => {
        const _: () = {
            #[unsafe(link_section = ".init_array")]
            #[used]
            static _FUNC: unsafe extern "C" fn() = {
                unsafe extern "C" fn ctor() {
                    $($body)*
                }

                ctor
            };
        };
    };
}

#[macro_export]
macro_rules! dtor {
    (
        unsafe extern "C" fn() {
            $($body:tt)*
        }
    ) => {
        $crate::ctor! {
            unsafe extern "C" fn() {
                unsafe extern "C" fn dtor() {
                    $($body)*
                }

                gcore::ctor::atexit(dtor);
            }
        }
    };
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __wasm_call_dtors() {
    let dtors = unsafe { static_ref!(DTORS) };
    for &(dtor, arg) in dtors {
        unsafe { dtor(arg) }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __cxa_atexit(func: Dtor, arg: *mut (), _dso: *mut ()) -> i32 {
    let dtors = unsafe { static_mut!(DTORS) };

    if let Err(_) = dtors.try_push((func, arg)) {
        return -1;
    }

    0
}

unsafe extern "C" fn call(arg: *mut ()) {
    let f: AtExitFn = unsafe { mem::transmute::<*mut (), AtExitFn>(arg) };
    unsafe { f() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atexit(func: AtExitFn) -> i32 {
    unsafe { __cxa_atexit(call, func as *mut (), ptr::null_mut()) }
}
