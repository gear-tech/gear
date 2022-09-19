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

//! Gear message processor.

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

extern crate alloc;

use gear_core::message::ExitCode;

pub mod common;
pub mod configs;
mod executor;
mod ext;
mod handler;
mod processor;

/// Error exit code.
pub const ERR_EXIT_CODE: ExitCode = 1;

/// Destination isn't available for the message.
///
/// These messages can be any of `init`,`handle`, `handle_reply`.
/// If the message is `init` it means either:
/// 1. Program tries to init program with non existing code hash.
/// 2. Program tries to init terminated program.
/// If the message is `handle` or `handle_reply` it means, that destination
/// was terminated while the message was in the queue.
pub const UNAVAILABLE_DEST_EXIT_CODE: ExitCode = 2;

/// A try to init again initialized, existing program.
pub const RE_INIT_EXIT_CODE: ExitCode = 3;

pub use executor::{calculate_gas_for_code, calculate_gas_for_program, execute_wasm};
pub use ext::{Ext, ProcessorContext, ProcessorError, ProcessorExt};
pub use handler::handle_journal;
pub use processor::{
    prepare, process, PrepareResult, PreparedMessageExecutionContext, ProcessExecutionContext,
};
