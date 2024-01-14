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

/// We currently support 3 panic handler profiles:
/// - `panic-handler`: it displays `<unknown>`
/// - `panic-message`: it displays `'{message}'`
/// - `panic-location`: it displays `'{message}', {location}`
///
/// How we get the panic message in different versions of Rust:
/// - In nightly Rust, we use `#![feature(panic_info_message)]` and the
///   [`write!`] macro.
/// - In stable Rust, we need to modify the default panic handler message
///   format.
///
/// Default panic handler message format (according to <https://github.com/rust-lang/rust/pull/112849>):
/// - Rust  <1.73: `panicked at '{message}', {location}`
/// - Rust >=1.73: `panicked at {location}:\n{message}`
///
/// We parse the output of `impl Display for PanicInfo<'_>` and
/// then convert it to custom format: `'{message}', {location}`.
#[cfg(target_arch = "wasm32")]
#[cfg(feature = "panic-handler")]
pub mod panic_handler {
    use crate::ext;
    use core::panic::PanicInfo;

    /// Minimal panic handler.
    #[cfg(not(any(feature = "panic-message", feature = "panic-location")))]
    #[panic_handler]
    pub fn panic(_: &PanicInfo) -> ! {
        ext::panic("<unknown>")
    }

    #[cfg(any(feature = "panic-message", feature = "panic-location"))]
    mod constants {
        /// Max amount of bytes allowed to be thrown as string explanation
        /// of the error.
        pub const TRIMMED_MAX_LEN: usize = 1024; //TODO: do not duplicate `gear_core::str::TRIMMED_MAX_LEN`
        /// This prefix is used to print debug message:
        /// `debug!("panic occurred: {msg}")`.
        pub const PANIC_OCCURRED: &str = "panic occurred: ";
        /// Size of array string that will be allocated on the stack.
        pub const ARRAY_STRING_MAX_LEN: usize = if cfg!(feature = "debug") {
            PANIC_OCCURRED.len()
        } else {
            0
        } + TRIMMED_MAX_LEN;
        /// This prefix is used by `impl Display for PanicInfo<'_>`.
        #[cfg(not(feature = "panic-info-message"))]
        pub const PANICKED_AT: &str = "panicked at ";
    }

    #[cfg(any(feature = "panic-message", feature = "panic-location"))]
    use constants::*;

    /// Panic handler for nightly Rust.
    #[cfg(feature = "panic-info-message")]
    #[cfg(any(feature = "panic-message", feature = "panic-location"))]
    #[panic_handler]
    pub fn panic(panic_info: &PanicInfo) -> ! {
        use crate::prelude::fmt::Write;
        use arrayvec::ArrayString;

        let mut debug_msg = ArrayString::<ARRAY_STRING_MAX_LEN>::new();

        #[cfg(feature = "debug")]
        let _ = debug_msg.try_push_str(PANIC_OCCURRED);

        let _ = match (panic_info.message(), panic_info.location()) {
            #[cfg(feature = "panic-location")]
            (Some(msg), Some(loc)) => write!(&mut debug_msg, "'{msg}', {loc}"),
            #[cfg(not(feature = "panic-location"))]
            (Some(msg), _) => write!(&mut debug_msg, "'{msg}'"),
            _ => ext::panic("<unknown>"),
        };

        #[cfg(feature = "debug")]
        let _ = ext::debug(&debug_msg);

        #[cfg(feature = "debug")]
        match debug_msg.get(PANIC_OCCURRED.len()..) {
            Some(msg) => ext::panic(msg),
            _ => ext::panic("<unknown>"),
        }

        #[cfg(not(feature = "debug"))]
        ext::panic(&debug_msg)
    }

    /// Panic handler for stable Rust >=1.73.
    #[cfg(not(feature = "panic-info-message"))]
    #[cfg(any(feature = "panic-message", feature = "panic-location"))]
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

        let mut debug_msg = ArrayString::<ARRAY_STRING_MAX_LEN>::new();

        #[cfg(feature = "debug")]
        let _ = debug_msg.try_push_str(PANIC_OCCURRED);

        #[cfg(feature = "panic-location")]
        for s in ["'", message, "', ", location] {
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

        #[cfg(feature = "debug")]
        match debug_msg.get(PANIC_OCCURRED.len()..) {
            Some(msg) => ext::panic(msg),
            _ => ext::panic("<unknown>"),
        }

        #[cfg(not(feature = "debug"))]
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
