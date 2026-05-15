// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
//! Gear programs run on Wasm, so the Rust program error is
//! considered equal to the Wasm runtime error `wasm32::unreachable`.
//! Panic handlers are available in two implementations -
//! debug and non-debug mode, for programs built in `wasm32` architecture.
//! For `debug` mode it provides more extensive logging.

#[cfg(target_arch = "wasm32")]
#[cfg(feature = "oom-handler")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    crate::ext::oom_panic()
}
/// We currently support 3 panic handler profiles:
/// - `panic-handler`: it displays `panicked with '<unknown>'`
/// - `panic-message`: it displays `panicked with '{message}'`
/// - `panic-location`: it displays `panicked with '{message}' at '{location}'`
#[cfg(target_arch = "wasm32")]
#[cfg(feature = "panic-handler")]
mod panic_handler {
    use crate::ext;
    use core::panic::PanicInfo;

    mod constants {
        /// This prefix is used before the panic message.
        pub const PANIC_PREFIX: &str = "panicked with ";
        /// This panic message is used in the minimal panic handler and when
        /// internal errors occur.
        #[cfg(not(feature = "panic-message"))]
        pub const UNKNOWN_REASON: &str = "<unknown>";
        /// Max amount of bytes allowed to be thrown as string explanation
        /// of the error.
        #[cfg(feature = "panic-message")]
        pub const TRIMMED_MAX_LEN: usize = 1024; //TODO: do not duplicate
        // `gear_core::str::TRIMMED_MAX_LEN`
    }

    use constants::*;

    /// Minimal panic handler.
    #[cfg(not(feature = "panic-message"))]
    #[panic_handler]
    pub fn panic(_: &PanicInfo) -> ! {
        const MESSAGE: &str = const_format::formatcp!("{PANIC_PREFIX}'{UNKNOWN_REASON}'");

        #[cfg(feature = "debug")]
        ext::debug(MESSAGE);

        ext::panic_str(MESSAGE)
    }

    /// Panic handler with extra information.
    #[cfg(feature = "panic-message")]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::Write;
        use arrayvec::ArrayString;

        let mut debug_msg = ArrayString::<TRIMMED_MAX_LEN>::new();
        let _ = debug_msg.try_push_str(PANIC_PREFIX);

        let msg = panic_info.message();

        #[cfg(feature = "panic-location")]
        if let Some(loc) = panic_info.location() {
            let _ = write!(&mut debug_msg, "'{msg}' at '{loc}'");
        }

        #[cfg(not(feature = "panic-location"))]
        let _ = write!(&mut debug_msg, "'{msg}'");

        #[cfg(feature = "debug")]
        ext::debug(&debug_msg);

        ext::panic_str(&debug_msg)
    }
}
