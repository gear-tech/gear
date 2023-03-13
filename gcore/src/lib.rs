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

//! Lightweight library for use in Gear programs.
//!
//! This library should be used as a standard library when writing Gear
//! programs. Compared to [`gstd`](https://docs.gear.rs/gstd/) crate, this
//! library provides lower-level primitives that allow you to develop less
//! expensive programs. Choose it if you are ready to write more code
//! but get a more efficient Wasm.
//!
//! Note that you are to define panic and out-of-memory handlers, as the crate
//! does not provide them by default.
//!
//! # Examples
//!
//! ```
//! #![no_std]
//! #![feature(alloc_error_handler)]
//!
//! extern crate galloc;
//!
//! use gcore::msg;
//!
//! #[no_mangle]
//! extern "C" fn handle() {
//!     let mut bytes = [0; 64];
//!     msg::read(&mut bytes).expect("Unable to read");
//!     if let Ok(payload) = core::str::from_utf8(&bytes) {
//!         if payload == "PING" {
//!             msg::reply(b"PONG", 0).expect("Unable to reply");
//!         }
//!     }
//! }
//!
//! # #[cfg(target = "wasm32")]
//! #[alloc_error_handler]
//! pub fn oom(_: core::alloc::Layout) -> ! {
//!     core::arch::wasm32::unreachable()
//! }
//!
//! # #[cfg(target = "wasm32")]
//! #[panic_handler]
//! fn panic(_: &core::panic::PanicInfo) -> ! {
//!     core::arch::wasm32::unreachable()
//! }
//!
//! # fn main() {}
//! ```

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

extern crate alloc;

pub mod errors;
pub mod exec;
pub mod msg;
pub mod prog;

mod general;
pub use general::*;

mod utils;
pub use utils::ext;

use core::mem::size_of;
use static_assertions::const_assert;

// This allows all casts from u32 into usize be safe.
const_assert!(size_of::<u32>() <= size_of::<usize>());
