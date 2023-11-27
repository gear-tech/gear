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

//! Module with custom panic handlers implementations.
//! Introduces Gear's implementation of panic for memory allocation
//! and other common Rust panic.
//! Gear smart contracts run on Wasm, so the Rust program error is
//! considered equal to the Wasm runtime error `wasm32::unreachable`.
//! Panic handlers are available in two implementations -
//! debug and non-debug mode, for programs built in `wasm32` architecture.
//! For `debug` mode it provides more extensive logging.

#[cfg(feature = "oom-handler")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    crate::ext::oom_panic()
}

#[cfg(feature = "panic-handler")]
mod panic_handler {
    use crate::ext;
    use core::panic::PanicInfo;

    #[cfg(not(feature = "debug"))]
    #[panic_handler]
    pub fn panic(_: &PanicInfo) -> ! {
        ext::panic("no info")
    }

    /// Panic handler for nightly Rust.
    #[cfg(feature = "debug")]
    #[cfg(feature = "panic-messages")]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::format;

        let message = panic_info.message();
        let msg = match (message, panic_info.location()) {
            (Some(msg), Some(loc)) => {
                format!("'{msg}', {}:{}:{}", loc.file(), loc.line(), loc.column())
            }
            (Some(msg), None) => format!("'{msg}'"),
            (None, Some(loc)) => {
                format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
            }
            _ => ext::panic("no info"),
        };

        crate::debug!("panic occurred: {msg}");
        ext::panic(&msg)
    }

    /// Panic handler for stable Rust.
    #[cfg(feature = "debug")]
    #[cfg(not(feature = "panic-messages"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::{borrow::Cow, format, ToString};

        // Default panic handler message format:
        // Rust  <1.73: `panicked at '{message}', {location}`
        // Rust >=1.73: `panicked at {location}:\n{message}`
        // source: https://github.com/rust-lang/rust/pull/112849

        const PANICKED_AT_LEN: usize = "panicked at ".len();

        let default_panic_msg = panic_info.to_string();
        let is_old_panic_format = default_panic_msg.as_bytes().get(PANICKED_AT_LEN) == Some(&b'\'');

        let maybe_panic_msg = if is_old_panic_format {
            default_panic_msg.get(PANICKED_AT_LEN..).map(Cow::Borrowed)
        } else {
            let mut iter = default_panic_msg.splitn(2, ":\n");
            iter.next().zip(iter.next()).and_then(|(line1, line2)| {
                let msg = line2;
                line1
                    .get(PANICKED_AT_LEN..line1.len())
                    .map(|location| Cow::Owned(format!("'{msg}', {location}")))
            })
        };

        if let Some(ref panic_msg) = maybe_panic_msg {
            let msg = panic_msg.as_ref();

            crate::debug!("panic occurred: {msg}");
            ext::panic(msg)
        } else {
            ext::panic("no info")
        }
    }
}
#[cfg(feature = "panic-handler")]
pub use panic_handler::*;
