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

#[cfg(not(feature = "ethexe"))]
use gprimitives::ReservationId;
use gprimitives::{ActorId, CodeId, MessageId};

pub(crate) trait AsRawPtr: AsRef<[u8]> + AsMut<[u8]> {
    fn as_ptr(&self) -> *const [u8; 32] {
        self.as_ref().as_ptr() as *const _
    }

    fn as_mut_ptr(&mut self) -> *mut [u8; 32] {
        self.as_mut().as_mut_ptr() as *mut _
    }
}

impl AsRawPtr for ActorId {}
impl AsRawPtr for CodeId {}
impl AsRawPtr for MessageId {}
#[cfg(not(feature = "ethexe"))]
impl AsRawPtr for ReservationId {}

/// Extensions for additional features.
pub mod ext {
    #[cfg(any(feature = "debug", debug_assertions))]
    use {
        crate::stack_buffer::{self, MAX_BUFFER_SIZE},
        core::{
            fmt::{self, Write as _},
            mem::MaybeUninit,
        },
    };

    /// Add a `data` string to the debug log.
    ///
    /// # Examples
    ///
    /// ```
    /// use gcore::ext;
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     ext::debug("Hello, world!");
    /// }
    /// ```
    #[cfg(any(feature = "debug", debug_assertions))]
    pub fn debug(data: &str) {
        unsafe { gsys::gr_debug(data.as_ptr(), data.len() as u32) }
    }

    /// Same as [`debug`] but uses a stack-allocated buffer.
    ///
    /// Note: message size is limited to
    /// [`MAX_BUFFER_SIZE`](crate::stack_buffer::MAX_BUFFER_SIZE).
    /// Message is truncated if it exceeds maximum buffer size.
    #[cfg(any(feature = "debug", debug_assertions))]
    pub fn stack_debug(args: fmt::Arguments<'_>) {
        struct StackFmtWriter<'a> {
            buf: &'a mut [MaybeUninit<u8>],
            pos: usize,
        }

        impl fmt::Write for StackFmtWriter<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                let upper_bound = (self.pos + s.len()).min(MAX_BUFFER_SIZE);
                if let Some(buf) = self.buf.get_mut(self.pos..upper_bound) {
                    let buf = buf as *mut [MaybeUninit<u8>] as *mut [u8];
                    let s = &s.as_bytes()[..buf.len()];

                    // SAFETY: we only write to uninitialized memory
                    unsafe {
                        (*buf).copy_from_slice(s);
                    }

                    self.pos += buf.len();
                }

                Ok(())
            }
        }

        stack_buffer::with_byte_buffer(MAX_BUFFER_SIZE, |buf| {
            let mut writer = StackFmtWriter { buf, pos: 0 };
            writer.write_fmt(args).expect("fmt failed");

            // SAFETY: buffer was initialized via `write_fmt` and limited by `pos`
            unsafe { gsys::gr_debug(writer.buf.as_ptr().cast(), writer.pos as u32) }
        });
    }

    /// Panic
    ///
    /// Can be used to pass some data to error reply payload.
    ///
    /// Function is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```
    /// use gcore::ext;
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     ext::panic(&[0, 1, 2, 3]);
    /// }
    /// ```
    pub fn panic(data: &[u8]) -> ! {
        unsafe { gsys::gr_panic(data.as_ptr(), data.len() as u32) }
    }

    /// Panic
    ///
    /// Function is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```
    /// use gcore::ext;
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     ext::panic_str("I decided to panic");
    /// }
    /// ```
    pub fn panic_str(data: &str) -> ! {
        panic(data.as_bytes())
    }

    /// Out of memory panic
    ///
    /// Function is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// #![no_std]
    /// #![feature(alloc_error_handler)]
    /// #![feature(allocator_api)]
    ///
    /// extern crate alloc;
    ///
    /// use alloc::alloc::{Global, Layout, Allocator};
    /// use gcore::ext;
    ///
    /// #[alloc_error_handler]
    /// fn oom(_layout: Layout) -> ! {
    ///     ext::oom_panic()
    /// }
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     let layout = Layout::new::<[u8; 64 * 1024]>();
    ///     if Global.allocate(layout).is_err() {
    ///         alloc::alloc::handle_alloc_error(layout);
    ///     }
    /// }
    /// ```
    pub fn oom_panic() -> ! {
        unsafe { gsys::gr_oom_panic() }
    }
}

/// Add a debug message to the log.
///
/// Debug messages are available only if the program is compiled
/// in debug mode.
///
/// ```shell
/// cargo build --debug
/// cargo build --features=debug
/// ```
///
/// You can see the debug messages when running the program using the `gtest`
/// crate. To see these messages when executing the program on the node, you
/// should run the node with the `RUST_LOG="gwasm=debug"` environment variable.
///
/// Note: message size is limited to
/// [`MAX_BUFFER_SIZE`](crate::stack_buffer::MAX_BUFFER_SIZE).
/// Message is truncated if it exceeds maximum buffer size.
///
/// If you need bigger message size, consider using `gstd::heap_debug!()` macro.
///
/// # Examples
///
/// ```
/// use gcore::debug;
///
/// #[unsafe(no_mangle)]
/// extern "C" fn handle() {
///     debug!("String literal");
///
///     let value = 42;
///     debug!("{value}");
///
///     debug!("Formatted: value = {value}");
/// }
/// ```
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($fmt:expr) => {
        $crate::ext::stack_debug(format_args!($fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::ext::stack_debug(format_args!($fmt, $($args)*))
    };
}

#[cfg(not(any(feature = "debug", debug_assertions)))]
#[allow(missing_docs)]
#[macro_export]
macro_rules! debug {
    ($fmt:expr) => {};
    ($fmt:expr, $($args:tt)*) => {};
}

/// Get mutable reference to `static mut`.
///
/// Note that getting two mutable references to `static mut` is UB.
///
/// ```
/// use gcore::static_mut;
///
/// static mut X: usize = 0;
///
/// let _: &mut usize = unsafe { static_mut!(X) };
/// ```
#[macro_export]
macro_rules! static_mut {
    ($expr:expr) => {{
        #[allow(clippy::deref_addrof)] // https://github.com/rust-lang/rust-clippy/issues/13783
        let ret: &mut _ = (&mut *&raw mut $expr);
        ret
    }};
}

/// Get shared reference to `static mut`.
///
/// ```
/// use gcore::static_ref;
///
/// static mut X: usize = 0;
///
/// let _: &usize = unsafe { static_ref!(X) };
/// ```
#[macro_export]
macro_rules! static_ref {
    ($expr:expr) => {{
        #[allow(clippy::deref_addrof)] // https://github.com/rust-lang/rust-clippy/issues/13783
        let ret: &_ = (&*&raw const $expr);
        ret
    }};
}
