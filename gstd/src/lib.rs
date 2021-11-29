// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![no_std]
#![cfg_attr(target_arch = "wasm32", feature(alloc_error_handler))]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

extern crate galloc;

mod async_runtime;
mod common;
pub mod exec;
pub mod lock;
pub mod macros;
pub mod msg;
pub mod prelude;

pub use async_runtime::{message_loop, record_reply};
pub use common::errors;
pub use common::handlers::*;
pub use common::primitives::*;
pub use gstd_codegen::async_main;

pub use prelude::*;

#[cfg(feature = "debug")]
pub use gcore::ext;
