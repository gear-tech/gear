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

//! Standard library for use in Gear programs.
//!
//! This library should be used as a standard library when writing Gear
//! programs. Compared to [`gcore`](https://docs.gear.rs/gcore/) crate,
//! this library provides higher-level primitives that allow you to develop more
//! complex dApps. Choose this library if you are ready to spend more gas but
//! receive refined code.
//!
//! `gstd` crate provides many advanced tools for a developer, such as
//! asynchronous programming primitives, arbitrary types encoding/decoding,
//! providing convenient instruments for creating programs from programs, etc.
//!
//! # Minimum supported Rust version
//! This crate requires **Rust >= 1.73** due to the implementation of the panic
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
//! #[no_mangle]
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
//! ```
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
#![warn(missing_docs)]
#![cfg_attr(
    all(
        target_arch = "wasm32",
        feature = "panic-info-message",
        feature = "panic-message"
    ),
    feature(panic_info_message)
)]
#![cfg_attr(
    all(target_arch = "wasm32", feature = "oom-handler"),
    feature(alloc_error_handler)
)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]
#![doc(test(attr(deny(warnings), allow(unused_variables, unused_assignments))))]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate galloc;

mod async_runtime;
mod common;
mod config;
pub mod critical;
pub mod exec;
mod macros;
pub mod msg;
pub mod prelude;
pub mod prog;
mod reservations;
pub mod sync;
pub mod util;

pub use async_runtime::{handle_signal, message_loop, record_reply};
pub use common::{errors, primitives::*};
pub use config::Config;
pub use gcore::{ext, BlockCount, BlockNumber, Gas, GasMultiplier, Percent, Value};
pub use gstd_codegen::{async_init, async_main};
pub use prelude::*;
pub use reservations::*;

// This allows all casts from u32 into usize be safe.
const _: () = assert!(core::mem::size_of::<u32>() <= core::mem::size_of::<usize>());
