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

//! Lightweight library for use in Gear programs.
//!
//! This library should be used as a standard library when writing Gear
//! programs. Compared to [`gstd`](https://docs.rs/gstd/) crate, this
//! library provides lower-level primitives that allow you to develop less
//! expensive programs. Choose it if you are ready to write more code
//! but get a more efficient Wasm.
//!
//! Note that you are to define panic and out-of-memory handlers, as the crate
//! does not provide them by default.
//!
//! # Examples
//!
//! ```rust,ignore
//! #![no_std]
//! #![feature(alloc_error_handler)]
//!
//! extern crate galloc;
//!
//! use gcore::msg;
//!
//! #[unsafe(no_mangle)]
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
//! # #[cfg(target_arch = "wasm32")]
//! #[alloc_error_handler]
//! pub fn oom(_: core::alloc::Layout) -> ! {
//!     core::arch::wasm32::unreachable()
//! }
//!
//! # #[cfg(target_arch = "wasm32")]
//! #[panic_handler]
//! fn panic(_: &core::panic::PanicInfo) -> ! {
//!     core::arch::wasm32::unreachable()
//! }
//! # fn main() {}
//! ```

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(test(attr(deny(warnings), allow(unused_variables, unused_assignments))))]

pub mod errors;
pub mod exec;
pub mod msg;
pub mod prog;
pub use gear_stack_buffer as stack_buffer;
#[cfg(not(feature = "ethexe"))]
pub use gprimitives::ReservationId;
pub use gprimitives::{ActorId, CodeId, MessageHandle, MessageId, Ss58Address};

mod utils;
pub use utils::ext;

pub use gsys::{BlockCount, BlockNumber, EnvVars, Gas, GasMultiplier, Percent, Value};

// This allows all casts from u32 into usize be safe.
const _: () = assert!(size_of::<u32>() <= size_of::<usize>());
