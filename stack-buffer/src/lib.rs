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

//! Stack allocations utils.

#![no_std]
#![feature(c_unwind)]

extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    mem::{ManuallyDrop, MaybeUninit},
    slice,
};

/// The maximum buffer size that can be allocated on the stack.
/// This is currently limited to 64 KiB.
pub const MAX_BUFFER_SIZE: usize = 64 * 1024;

/// A closure data type that is used in the native library to pass
/// a pointer to allocated stack memory.
type Callback = unsafe extern "C-unwind" fn(ptr: *mut MaybeUninit<u8>, data: *mut c_void);

#[cfg(any(
    feature = "compile-alloca",
    all(not(feature = "compile-alloca"), target_arch = "wasm32")
))]
extern "C-unwind" {
    /// Function from the native library that manipulates the stack pointer directly.
    /// Can be used to dynamically allocate stack space.
    fn c_with_alloca(size: usize, callback: Callback, data: *mut c_void);
}

/// This is a polyfill function that is used when the native library is unavailable.
/// The maximum size that can be allocated on the stack is limited
/// by the [`MAX_BUFFER_SIZE`] constant.
#[cfg(all(not(feature = "compile-alloca"), not(target_arch = "wasm32")))]
unsafe extern "C-unwind" fn c_with_alloca(_size: usize, callback: Callback, data: *mut c_void) {
    // Same as `MaybeUninit::uninit_array()`.
    // Create an uninitialized array of `MaybeUninit`. The `assume_init` is
    // safe because the type we are claiming to have initialized here is a
    // bunch of `MaybeUninit`s, which do not require initialization.
    let mut buffer = MaybeUninit::<[MaybeUninit<u8>; MAX_BUFFER_SIZE]>::uninit().assume_init();
    callback(buffer.as_mut_ptr(), data);
}

/// Helper function to create a trampoline between C and Rust code.
#[inline(always)]
fn get_trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(_closure: &F) -> Callback {
    trampoline::<F>
}

/// A function that serves as a trampoline between C and Rust code.
/// It is mainly used to switch from `fn()` to `FnOnce()`,
/// which allows local variables to be captured.
unsafe extern "C-unwind" fn trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(
    ptr: *mut MaybeUninit<u8>,
    data: *mut c_void,
) {
    // This code gets `*mut ManuallyDrop<F>`, then takes ownership of the `F` function
    // and executes it with a pointer to the allocated stack memory.
    let f = ManuallyDrop::take(&mut *(data as *mut ManuallyDrop<F>));
    f(ptr);
}

/// This is a higher-level function for dynamically allocating space on the stack.
fn with_alloca<T>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> T) -> T {
    let mut ret = MaybeUninit::uninit();

    let closure = |ptr| {
        let slice = unsafe { slice::from_raw_parts_mut(ptr, size) };
        ret.write(f(slice));
    };

    // The `closure` variable is passed as `*mut ManuallyDrop<F>` to the trampoline function.
    let trampoline = get_trampoline(&closure);
    let mut closure_data = ManuallyDrop::new(closure);

    unsafe {
        c_with_alloca(size, trampoline, &mut closure_data as *mut _ as *mut c_void);
        ret.assume_init()
    }
}

/// Calls function `f` with provided uninitialized byte buffer allocated on stack.
/// ### IMPORTANT
/// If buffer size is too big (currently bigger than 0x10000 bytes),
/// then allocation will be on heap.
/// If buffer is small enough to be allocated on stack, then real allocated
/// buffer size will be `size` aligned to 16 bytes.
pub fn with_byte_buffer<T>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> T) -> T {
    if size <= MAX_BUFFER_SIZE {
        with_alloca(size, f)
    } else {
        f(Vec::with_capacity(size).spare_capacity_mut())
    }
}
