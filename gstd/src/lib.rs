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

//! Declares modules, attributes, public re-exports.
//! Gear libs are `#![no_std]`, which makes them lightweight.

#![no_std]
#![cfg_attr(target_arch = "wasm32", feature(alloc_error_handler))]
#![cfg_attr(
    all(target_arch = "wasm32", feature = "debug"),
    feature(panic_info_message)
)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

#[cfg(target_arch = "wasm32")]
extern crate galloc;

mod async_runtime;
mod common;
pub mod exec;
pub mod lock;
pub mod macros;
pub mod msg;
pub mod prelude;
pub mod prog;

pub use async_runtime::{message_loop, record_reply};
pub use common::{errors, handlers::*, primitives::*};
pub use gstd_codegen::{async_init, async_main};
pub use macros::util;

pub use prelude::*;

#[cfg(feature = "debug")]
pub use gcore::ext;

/// Shared traits for gear programs.
pub mod traits {
    pub use crate::async_runtime::Wait;
}

pub use config::Config;

/// This module is for configuring `gstd` inside gear programs.
pub mod config {
    /// `gstd` configuration
    pub struct Config {
        pub wait_duration: u32,
    }

    impl Config {
        const fn default() -> Self {
            Self { wait_duration: 100 }
        }

        /// Get wait duration
        pub fn wait_duration() -> u32 {
            unsafe { CONFIG.wait_duration }
        }

        /// Set wait duration
        pub fn set_wait_duration(duration: u32) -> Result<(), ()> {
            // # TODO
            //
            // check duration
            unsafe { CONFIG.wait_duration = duration };
            Ok(())
        }
    }

    /// Private `gstd` configuration, only could be modified
    /// with the public interfaces of `Config`.
    static mut CONFIG: Config = Config::default();
}
