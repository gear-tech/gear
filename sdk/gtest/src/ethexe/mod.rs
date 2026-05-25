// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

mod api;
mod backend;
mod run;
mod runtime;

pub use api::{Program, ProgramBuilder, System};
pub(crate) use backend::EthexeBackend;

pub(crate) fn init_lazy_pages() {
    runtime::init_lazy_pages();
}

#[cfg(test)]
mod tests;
