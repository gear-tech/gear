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

//! Module with custom panic handlers implementations.
//! Introduces Gear's implementation of panic for memory allocation
//! and other common Rust panic.
//! Gear smart contracts run on Wasm, so the Rust program error is
//! considered equal to the Wasm runtime error `wasm32::unreachable`.
//! Panic handlers are available in two implementations -
//! debug and non-debug mode, for programs built in `wasm32` architecture.
//! For `debug` mode it provides more extensive logging.

#[cfg(target_arch = "wasm32")]
use {crate::ext, core::alloc::Layout, core::panic::PanicInfo};

#[cfg(target_arch = "wasm32")]
#[alloc_error_handler]
pub fn oom(_: Layout) -> ! {
    ext::oom_panic()
}

#[cfg(not(feature = "debug"))]
#[cfg(not(debug_assertions))]
#[cfg(target_arch = "wasm32")]
#[panic_handler]
pub fn panic(_: &PanicInfo) -> ! {
    ext::panic("no info")
}

#[cfg(any(feature = "debug", debug_assertions))]
#[cfg(target_arch = "wasm32")]
#[panic_handler]
pub fn panic(panic_info: &PanicInfo) -> ! {
    use crate::prelude::format;

    let msg = match (panic_info.message(), panic_info.location()) {
        (Some(msg), Some(loc)) => format!(
            "'{:?}', {}:{}:{}",
            msg,
            loc.file(),
            loc.line(),
            loc.column()
        ),
        (Some(msg), None) => format!("'{msg:?}'"),
        (None, Some(loc)) => {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        }
        _ => ext::panic("no info"),
    };

    ext::panic(&msg)
}
