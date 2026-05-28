// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear message processor.

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

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
    AllocExtError, Ext, ExtInfo, FallibleExtError, ProcessorContext, ProcessorExternalities,
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
