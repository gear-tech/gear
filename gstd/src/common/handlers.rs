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

    #[cfg(feature = "debug")]
    mod constants {
        pub const TRIMMED_MAX_LEN: usize = 1024; //TODO: find better way to access it
        pub const PANIC_OCCURRED: &str = "panic occurred: ";
        #[cfg(not(feature = "panic-messages"))]
        pub const PANICKED_AT: &str = "panicked at ";
    }

    #[cfg(feature = "debug")]
    use constants::*;

    /// Panic handler for nightly Rust.
    #[cfg(feature = "debug")]
    #[cfg(feature = "panic-messages")]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::Write;
        use arrayvec::ArrayString;

        let option = panic_info.message().zip(panic_info.location());
        let (message, location) = unsafe { option.unwrap_unchecked() };

        let mut debug_msg = ArrayString::<{ PANIC_OCCURRED.len() + TRIMMED_MAX_LEN }>::new();
        let _ = write!(&mut debug_msg, "{PANIC_OCCURRED}'{message}', {location}");

        let _ = ext::debug(&debug_msg);

        let msg = unsafe { debug_msg.get_unchecked(PANIC_OCCURRED.len()..) };
        ext::panic(&msg)
    }

    // Default panic handler message format:
    // Rust  <1.73: `panicked at '{message}', {location}`
    // Rust >=1.73: `panicked at {location}:\n{message}`
    // source: https://github.com/rust-lang/rust/pull/112849

    /// Panic handler for stable Rust <1.73.
    #[rustversion::before(1.73)]
    #[cfg(feature = "debug")]
    #[cfg(not(feature = "panic-messages"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::Write;
        use arrayvec::ArrayString;

        static_assertions::const_assert!(PANICKED_AT.len() == (PANIC_OCCURRED.len() - 4));

        let mut debug_msg = ArrayString::<{ PANIC_OCCURRED.len() + TRIMMED_MAX_LEN }>::new();

        let _ = debug_msg.try_push_str(&PANIC_OCCURRED[..4]);
        let _ = write!(&mut debug_msg, "{panic_info}");

        let src = (&PANIC_OCCURRED[4..]).as_bytes();
        let dest = unsafe {
            debug_msg
                .as_bytes_mut()
                .get_unchecked_mut(4..PANIC_OCCURRED.len())
        };
        dest.copy_from_slice(src);

        let _ = ext::debug(&debug_msg);

        let msg = unsafe { debug_msg.get_unchecked(PANIC_OCCURRED.len()..) };
        ext::panic(&msg)
    }

    /// Panic handler for stable Rust >=1.73.
    #[rustversion::since(1.73)]
    #[cfg(feature = "debug")]
    #[cfg(not(feature = "panic-messages"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::{fmt::Write, str};
        use arrayvec::ArrayString;

        let mut default_panic_msg = ArrayString::<{ PANICKED_AT.len() + TRIMMED_MAX_LEN }>::new();
        let _ = write!(&mut default_panic_msg, "{panic_info}");

        fn parse_panic_msg(msg: &str) -> Option<(&str, &str)> {
            const NEEDLE: [u8; 2] = *b":\n";

            let msg_bytes = msg.as_bytes();
            msg_bytes
                .windows(NEEDLE.len())
                .position(|window| NEEDLE.eq(window))
                .map(|pos| unsafe {
                    (
                        str::from_utf8_unchecked(msg_bytes.get_unchecked(PANICKED_AT.len()..pos)),
                        str::from_utf8_unchecked(msg_bytes.get_unchecked((pos + NEEDLE.len())..)),
                    )
                })
        }

        let option = parse_panic_msg(&default_panic_msg);
        let (location, message) = unsafe { option.unwrap_unchecked() };

        let mut debug_msg = ArrayString::<{ PANIC_OCCURRED.len() + TRIMMED_MAX_LEN }>::new();

        let _ = debug_msg.try_push_str(PANIC_OCCURRED);
        let _ = debug_msg.try_push('\'');
        let _ = debug_msg.try_push_str(message);
        let _ = debug_msg.try_push_str("', ");
        let _ = debug_msg.try_push_str(location);

        let _ = ext::debug(&debug_msg);

        let msg = unsafe { debug_msg.get_unchecked(PANIC_OCCURRED.len()..) };
        ext::panic(&msg)
    }
}
#[cfg(feature = "panic-handler")]
pub use panic_handler::*;
