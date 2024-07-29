// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
///
/// How we get the panic message in different versions of Rust:
/// - In nightly Rust, we use `#![feature(panic_info_message)]` and the
///   [`write!`] macro.
/// - In stable Rust, we need to modify the default panic handler message
///   format.
///
/// Default panic handler message format (according to <https://github.com/rust-lang/rust/pull/112849>):
/// `panicked at {location}:\n{message}`
///
/// We parse the output of `impl Display for PanicInfo<'_>` and
/// then convert it to custom format:
/// `panicked with '{message}'[ at '{location}']`.
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

        /// This prefix is used by `impl Display for PanicInfo<'_>`.
        #[cfg(all(not(feature = "panic-info-message"), feature = "panic-message"))]
        pub const PANICKED_AT: &str = "panicked at ";

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
        let _ = ext::debug(MESSAGE);

        ext::panic(MESSAGE)
    }

    /// Panic handler for nightly Rust.
    #[cfg(all(feature = "panic-info-message", feature = "panic-message"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::Write;
        use arrayvec::ArrayString;

        let mut debug_msg = ArrayString::<TRIMMED_MAX_LEN>::new();
        let _ = debug_msg.try_push_str(PANIC_PREFIX);

        let msg = panic_info.message();

        #[allow(unused_variables)]
        if let Some(loc) = panic_info.location() {
            #[cfg(feature = "panic-location")]
            let _ = write!(&mut debug_msg, "'{msg}' at '{loc}'");
        } else {
            #[cfg(not(feature = "panic-location"))]
            let _ = write!(&mut debug_msg, "'{msg}'");
        }

        #[cfg(feature = "debug")]
        let _ = ext::debug(&debug_msg);

        ext::panic(&debug_msg)
    }

    /// Panic handler for stable Rust.
    #[cfg(all(not(feature = "panic-info-message"), feature = "panic-message"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::{self, Write};
        use arrayvec::ArrayString;

        #[derive(Default)]
        struct TempBuffer<const CAP: usize> {
            overflowed: bool,
            buffer: ArrayString<CAP>,
        }

        impl<const CAP: usize> TempBuffer<CAP> {
            #[inline]
            fn write_str(&mut self, s: &str) {
                if !self.overflowed && self.buffer.write_str(s).is_err() {
                    self.overflowed = true;
                }
            }
        }

        #[derive(Default)]
        struct TempOutput {
            found_prefix: bool,
            found_delimiter: bool,
            #[cfg(feature = "panic-location")]
            location: TempBuffer<TRIMMED_MAX_LEN>,
            message: TempBuffer<TRIMMED_MAX_LEN>,
        }

        impl fmt::Write for TempOutput {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                if !self.found_prefix && s.len() == PANICKED_AT.len() {
                    self.found_prefix = true;
                    return Ok(());
                }

                if !self.found_delimiter {
                    if s == ":\n" {
                        self.found_delimiter = true;
                        return Ok(());
                    }
                    #[cfg(feature = "panic-location")]
                    self.location.write_str(s);
                } else {
                    self.message.write_str(s);
                }

                Ok(())
            }
        }

        let mut output = TempOutput::default();
        let _ = write!(&mut output, "{panic_info}");

        #[cfg(feature = "panic-location")]
        let location = &*output.location.buffer;
        let message = &*output.message.buffer;

        let mut debug_msg = ArrayString::<TRIMMED_MAX_LEN>::new();
        let _ = debug_msg.try_push_str(PANIC_PREFIX);

        #[cfg(feature = "panic-location")]
        for s in ["'", message, "' at '", location, "'"] {
            if debug_msg.try_push_str(s).is_err() {
                break;
            }
        }

        #[cfg(not(feature = "panic-location"))]
        for s in ["'", message, "'"] {
            if debug_msg.try_push_str(s).is_err() {
                break;
            }
        }

        #[cfg(feature = "debug")]
        let _ = ext::debug(&debug_msg);

        ext::panic(&debug_msg)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{format, panic, prelude::v1::*};

    /// Here is a test to verify that the default panic handler message
    /// format has not changed.
    #[test]
    fn panic_msg_format_not_changed() {
        const MESSAGE: &str = "message";

        panic::set_hook(Box::new(|panic_info| {
            let location = panic_info.location().unwrap();
            assert_eq!(
                panic_info.to_string(),
                format!("panicked at {location}:\n{MESSAGE}")
            );
        }));

        let result = panic::catch_unwind(|| {
            panic!("{MESSAGE}");
        });
        assert!(result.is_err());
    }
}
