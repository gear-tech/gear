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

#[cfg(target_arch = "wasm32")]
#[cfg(feature = "oom-handler")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    crate::ext::oom_panic()
}

/// We currently support 2 panic handler modes:
/// - non-debug: it prints `no info`
/// - debug: it prints `'{message}', {location}`
///   - In nightly Rust, we use `#![feature(panic_info_message)]` and the
///     [`write!`] macro.
///   - In stable Rust, we need to modify the default panic handler message
///     format.
///
/// Default panic handler message format (according to <https://github.com/rust-lang/rust/pull/112849>):
/// - Rust  <1.73: `panicked at '{message}', {location}`
/// - Rust >=1.73: `panicked at {location}:\n{message}`
///
/// We parse the output of `impl Display for PanicInfo<'_>` and
/// then convert it to custom format: `'{message}', {location}`.
///
/// Here is a test to verify that the default panic handler message format
/// has not changed:
/// ```
/// use std::panic::{self, PanicInfo};
///
/// const MESSAGE: &str = "message";
///
/// #[rustversion::before(1.73)]
/// fn expected_format(panic_info: &PanicInfo) -> String {
///     let location = panic_info.location().unwrap();
///     format!("panicked at '{MESSAGE}', {location}")
/// }
///
/// #[rustversion::since(1.73)]
/// fn expected_format(panic_info: &PanicInfo) -> String {
///     let location = panic_info.location().unwrap();
///     format!("panicked at {location}:\n{MESSAGE}")
/// }
///
/// panic::set_hook(Box::new(|panic_info| {
///     assert_eq!(panic_info.to_string(), expected_format(panic_info));
/// }));
///
/// let result = panic::catch_unwind(|| {
///     panic!("{MESSAGE}");
/// });
/// assert!(result.is_err());
/// ```
pub mod panic_handler {
    #[cfg(target_arch = "wasm32")]
    mod internal {
        use crate::ext;
        use core::panic::PanicInfo;

        /// Panic handler when debug feature is disabled.
        #[cfg(not(feature = "debug"))]
        #[panic_handler]
        pub fn panic(_: &PanicInfo) -> ! {
            ext::panic("no info")
        }

        #[cfg(feature = "debug")]
        mod constants {
            /// Max amount of bytes allowed to be thrown as string explanation
            /// of the error.
            pub const TRIMMED_MAX_LEN: usize = 1024; //TODO: do not duplicate `gear_core::str::TRIMMED_MAX_LEN`
            /// This prefix is used to print debug message:
            /// `debug!("panic occurred: {msg}")`.
            pub const PANIC_OCCURRED: &str = "panic occurred: ";
            /// This prefix is used by `impl Display for PanicInfo<'_>`.
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
            use crate::prelude::{fmt::Write, mem::MaybeUninit, str};
            use arrayvec::ArrayString;

            // SAFETY: The current implementation always returns Some.
            // https://github.com/rust-lang/rust/blob/5b8bc568d28b2e922290c9a966b3231d0ce9398b/library/std/src/panicking.rs#L643-L644
            let option = panic_info.message().zip(panic_info.location());
            let (message, location) = unsafe { option.unwrap_unchecked() };

            /// Maximum number of digits in `u32`.
            const ITOA_U32_BUF_SIZE: usize = 10;

            /// Converts `u32` to `&str`, `&str` is written to temp buffer.
            ///
            /// We use our own function because `impl Display for u32` is very
            /// large in WASM binary format (~2.5 KiB).
            fn itoa_u32(buffer: &mut [MaybeUninit<u8>; ITOA_U32_BUF_SIZE], mut n: u32) -> &str {
                let mut idx = buffer.len();
                loop {
                    // SAFETY: The bounds are always correct because this loop iterates over each
                    // digit in `u32`, and the maximum number of digits is defined in
                    // `ITOA_U32_BUF_SIZE` constant.
                    idx -= 1;
                    unsafe { buffer.get_unchecked_mut(idx) }.write((n % 10) as u8 + b'0');
                    n /= 10;
                    if n == 0 {
                        break;
                    }
                }
                // SAFETY: Since we are using a loop with a postcondition, the boundaries will
                // always be: `idx < buffer.len()`, i.e. we can do `get_unchecked(idx)`.
                // The expression `&*(buffer as *const [_] as *const _)` is equivalent to
                // `MaybeUninit::slice_assume_init_ref(&buffer[..idx])` and it gets the
                // initialized part of `buffer`.
                // Since the initialized part is filled with ascii digits, we can do
                // `str::from_utf8_unchecked`.
                unsafe {
                    str::from_utf8_unchecked(
                        &*(buffer.get_unchecked(idx..) as *const [_] as *const _),
                    )
                }
            }

            let mut debug_msg = ArrayString::<{ PANIC_OCCURRED.len() + TRIMMED_MAX_LEN }>::new();

            let _ = debug_msg.try_push_str(PANIC_OCCURRED);
            if write!(&mut debug_msg, "'{message}', ").is_ok() {
                for s in [
                    location.file(),
                    ":",
                    itoa_u32(
                        &mut [MaybeUninit::uninit(); ITOA_U32_BUF_SIZE],
                        location.line(),
                    ),
                    ":",
                    itoa_u32(
                        &mut [MaybeUninit::uninit(); ITOA_U32_BUF_SIZE],
                        location.column(),
                    ),
                ] {
                    if debug_msg.try_push_str(s).is_err() {
                        break;
                    }
                }
            }

            let _ = ext::debug(&debug_msg);

            // SAFETY: `debug_msg` is guaranteed to be initialized since `try_push_str` does
            // `memcpy`. If `memcpy` fails (e.g. isn't enough stack), the program will be
            // aborted by the executor with unreachable instruction.
            let msg = unsafe { debug_msg.get_unchecked(PANIC_OCCURRED.len()..) };
            ext::panic(&msg)
        }

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

            // SAFETY: `debug_msg.len() >= PANIC_OCCURRED.len()` because `try_push_str`
            // pushes string `"pani"` and `write!()` pushes string `"panicked at "`.
            // The capacity of `debug_msg` is always enough to do this.
            unsafe {
                debug_msg
                    .as_bytes_mut()
                    .get_unchecked_mut(4..PANIC_OCCURRED.len())
                    .copy_from_slice(&PANIC_OCCURRED[4..].as_bytes());
            }

            let _ = ext::debug(&debug_msg);

            // SAFETY: `debug_msg` is guaranteed to be initialized since `try_push_str` does
            // `memcpy` (see panic handler for nightly rust for more details).
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

            let mut default_panic_msg =
                ArrayString::<{ PANICKED_AT.len() + TRIMMED_MAX_LEN }>::new();
            let _ = write!(&mut default_panic_msg, "{panic_info}");

            fn parse_panic_msg(msg: &str) -> Option<(&str, &str)> {
                const NEEDLE: [u8; 2] = *b":\n";

                // SAFETY: We can use `str::from_utf8_unchecked` because the delimiter is ascii
                // characters. Therefore, the strings between the delimiter will be a valid
                // UTF-8 sequence (see https://en.wikipedia.org/wiki/UTF-8#Encoding).
                let msg_bytes = msg.as_bytes();
                msg_bytes
                    .windows(NEEDLE.len())
                    .position(|window| NEEDLE.eq(window))
                    .map(|pos| unsafe {
                        (
                            str::from_utf8_unchecked(
                                msg_bytes.get_unchecked(PANICKED_AT.len()..pos),
                            ),
                            str::from_utf8_unchecked(
                                msg_bytes.get_unchecked((pos + NEEDLE.len())..),
                            ),
                        )
                    })
            }

            // FIXME
            let option = parse_panic_msg(&default_panic_msg);
            let (location, message) = unsafe { option.unwrap_unchecked() };

            let mut debug_msg = ArrayString::<{ PANIC_OCCURRED.len() + TRIMMED_MAX_LEN }>::new();

            let _ = debug_msg.try_push_str(PANIC_OCCURRED);
            let _ = debug_msg.try_push_str("'");
            let _ = debug_msg.try_push_str(message);
            let _ = debug_msg.try_push_str("', ");
            let _ = debug_msg.try_push_str(location);

            let _ = ext::debug(&debug_msg);

            // SAFETY: `debug_msg` is guaranteed to be initialized since `try_push_str` does
            // `memcpy` (see panic handler for nightly rust for more details).
            let msg = unsafe { debug_msg.get_unchecked(PANIC_OCCURRED.len()..) };
            ext::panic(&msg)
        }
    }
}
