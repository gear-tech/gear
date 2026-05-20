// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod cli;
mod command;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::*;

pub use cli::*;
pub use command::*;
pub use sc_cli::{Error, Result};

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
