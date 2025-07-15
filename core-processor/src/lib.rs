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

//! Gear message processor.

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

extern crate alloc;

pub mod common;
pub mod configs;
mod context;
mod executor;
mod ext;
mod handler;
pub mod precharge;
mod processing;

pub use context::{ProcessExecutionContext, SystemReservationContext};
pub use ext::{
    AllocExtError, Ext, FallibleExtError, ProcessorContext, ProcessorExternalities,
    UnrecoverableExtError,
};
pub use handler::handle_journal;
pub use precharge::*;
pub use processing::{
    process, process_allowance_exceed, process_code_not_exists, process_execution_error,
    process_failed_init, process_instrumentation_failed, process_program_exited,
    process_reinstrumentation_error, process_success, process_uninitialized,
};

/// Informational functions for core-processor and executor.
pub mod informational {
    pub use crate::executor::execute_for_reply;
}
