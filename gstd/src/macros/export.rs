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

//! Gear `export!` macro creates FFI function that returns a pointer to the
//! Wasm memory and the length of the data required to export.
//! It enables, for example, JS applications to get data from Wasm.

/// Create an FFI function that returns some data from the program as a fat
/// pointer to the Wasm memory.
///
/// The value provided should have a `to_string` method (e.g., implement the
/// [`Display`](https://doc.rust-lang.org/std/fmt/trait.Display.html) trait).
///
/// It enables, for example, JS applications to get data from Wasm.
///
/// # Examples
///
/// ```
/// use gstd::export;
///
/// static mut VALUE: i32 = 0;
///
/// export!(my_function -> unsafe { VALUE });
///
/// #[no_mangle]
/// extern "C" fn init() {
///     unsafe { VALUE = 42 };
/// }
/// ```
#[macro_export]
macro_rules! export {
    ($f:ident -> $val:expr) => {
        #[no_mangle]
        extern "C" fn $f() -> *mut [i32; 2] {
            let buffer = $val.to_string();
            let result = $crate::util::to_wasm_ptr(buffer.as_bytes());
            core::mem::forget(buffer);
            result
        }
    };
}
