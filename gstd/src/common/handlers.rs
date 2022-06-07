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

#[cfg(not(feature = "debug"))]
#[cfg(target_arch = "wasm32")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    core::arch::wasm32::unreachable()
}

#[cfg(feature = "debug")]
#[cfg(target_arch = "wasm32")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    crate::debug!("Runtime memory exhausted. Aborting");
    core::arch::wasm32::unreachable()
}

#[cfg(not(feature = "debug"))]
#[cfg(target_arch = "wasm32")]
#[panic_handler]
pub fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}

#[cfg(feature = "debug")]
#[cfg(target_arch = "wasm32")]
#[panic_handler]
pub fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    match (panic_info.message(), panic_info.location()) {
        (Some(msg), Some(loc)) => crate::debug!(
            "panic occurred: '{:?}', {}:{}:{}",
            msg,
            loc.file(),
            loc.line(),
            loc.column()
        ),
        (Some(msg), None) => crate::debug!("panic occurred: '{:?}'", msg),
        (None, Some(loc)) => {
            crate::debug!(
                "panic occurred: {}:{}:{}",
                loc.file(),
                loc.line(),
                loc.column()
            )
        }
        _ => crate::debug!("panic occurred: no info"),
    }

    core::arch::wasm32::unreachable();
}
