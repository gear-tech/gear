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

//! Standard library for use in Gear programs.
//!
//! This library should be used as a standard library when writing Gear
//! programs. Compared to [`gcore`](https://docs.rs/gcore/) crate,
//! this library provides higher-level primitives that allow you to develop more
//! complex dApps. Choose this library if you are ready to spend more gas but
//! receive refined code.
//!
//! `gstd` crate provides many advanced tools for a developer, such as
//! asynchronous programming primitives, arbitrary types encoding/decoding,
//! providing convenient instruments for creating programs from programs, etc.
//!
//! # Minimum supported Rust version
//! This crate requires **Rust >= 1.81** due to the implementation of the panic
//! handler in the stable version.
//!
//! # Crate features
#![cfg_attr(
    feature = "document-features",
    cfg_attr(doc, doc = ::document_features::document_features!())
)]
//! # Examples
//!
//! Decode input payload using a custom type:
//!
//! ```
//! # const _: &'static str = stringify! {
//! #![no_std]
//! # };
//!
//! use gstd::{msg, prelude::*};
//!
//! #[derive(Decode, Encode, TypeInfo)]
//! #[codec(crate = gstd::codec)]
//! #[scale_info(crate = gstd::scale_info)]
//! struct Payload {
//!     question: String,
//!     answer: u8,
//! }
//!
//! #[unsafe(no_mangle)]
//! extern "C" fn handle() {
//!     let payload: Payload = msg::load().expect("Unable to decode payload");
//!     if payload.question == "life-universe-everything" {
//!         msg::reply(payload.answer, 0).expect("Unable to reply");
//!     }
//! }
//! ```
//!
//! Asynchronous program example.
//!
//! It sends empty messages to three addresses and waits for at least two
//! replies ("approvals") during initialization. When invoked, it handles only
//! `PING` messages and sends empty messages to the three addresses, and waits
//! for just one approval. If approval is obtained, the program replies with
//! `PONG`.
//!
//! ```ignored
//! # const _: &'static str = stringify! {
//! #![no_std]
//! # };
//! use futures::future;
//! use gstd::{msg, prelude::*, ActorId};
//!
//! static mut APPROVERS: [ActorId; 3] = [ActorId::zero(); 3];
//!
//! #[derive(Debug, Decode, TypeInfo)]
//! #[codec(crate = gstd::codec)]
//! #[scale_info(crate = gstd::scale_info)]
//! pub struct Input {
//!     pub approvers: [ActorId; 3],
//! }
//!
//! #[gstd::async_init]
//! async fn init() {
//!     let payload: Input = msg::load().expect("Failed to decode input");
//!     unsafe { APPROVERS = payload.approvers };
//!
//!     let mut requests: Vec<_> = unsafe { APPROVERS }
//!         .iter()
//!         .map(|addr| msg::send_bytes_for_reply(*addr, b"", 0, 0))
//!         .collect::<Result<_, _>>()
//!         .unwrap();
//!
//!     let mut threshold = 0;
//!     while !requests.is_empty() {
//!         let (.., remaining) = future::select_all(requests).await;
//!         threshold += 1;
//!         if threshold >= 2 {
//!             break;
//!         }
//!         requests = remaining;
//!     }
//! }
//!
//! #[gstd::async_main]
//! async fn main() {
//!     let message = msg::load_bytes().expect("Failed to load payload bytes");
//!     if message != b"PING" {
//!         return;
//!     }
//!
//!     let requests: Vec<_> = unsafe { APPROVERS }
//!         .iter()
//!         .map(|addr| msg::send_bytes_for_reply(*addr, b"", 0, 0))
//!         .collect::<Result<_, _>>()
//!         .unwrap();
//!
//!     _ = future::select_all(requests).await;
//!     msg::reply(b"PONG", 0).expect("Unable to reply");
//! }
//! # fn main() {}
//! ```

#![no_std]
#![cfg_attr(
    all(target_arch = "wasm32", feature = "oom-handler"),
    feature(alloc_error_handler)
)]
#![allow(ambiguous_glob_reexports)]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(test(attr(deny(warnings), allow(unused_variables, unused_assignments))))]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate galloc;

mod async_runtime;
mod common;
mod config;
#[cfg(not(feature = "ethexe"))]
pub mod critical;
pub mod exec;
mod macros;
pub mod msg;
pub mod prelude;
pub mod prog;
#[cfg(not(feature = "ethexe"))]
mod reservations;
pub mod sync;
pub mod util;

pub use async_runtime::{handle_reply_with_hook, message_loop};
pub use common::errors;
pub use config::{Config, SYSTEM_RESERVE};
pub use gcore::{
    ActorId, BlockCount, BlockNumber, CodeId, EnvVars, Gas, GasMultiplier, MessageId, Percent,
    Ss58Address, Value, debug, static_mut, static_ref,
};
pub use gstd_codegen::{actor_id, async_init, async_main};
pub use prelude::*;

#[cfg(not(feature = "ethexe"))]
pub use {
    async_runtime::handle_signal, common::primitives_ext::*, gcore::ReservationId, reservations::*,
};

/// Extensions for additional features.
pub mod ext {
    pub use gcore::ext::*;

    use parity_scale_codec::Encode;

    /// Panic
    ///
    /// Can be used to pass some data to error reply payload.
    ///
    /// Syscall this function uses is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```
    /// use gstd::ext;
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     ext::panic((1, "important data"));
    /// }
    /// ```
    pub fn panic<T: Encode>(data: T) -> ! {
        panic_bytes(data.encode())
    }

    /// Panic
    ///
    /// Can be used to pass some data to error reply payload.
    ///
    /// Syscall this function uses is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```
    /// use gstd::ext;
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     ext::panic_bytes([0, 1, 2, 3]);
    /// }
    /// ```
    pub fn panic_bytes<T: AsRef<[u8]>>(data: T) -> ! {
        gcore::ext::panic(data.as_ref())
    }
}

// This allows all casts from u32 into usize be safe.
const _: () = assert!(size_of::<u32>() <= size_of::<usize>());
