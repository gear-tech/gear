// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This module is used to instrument a Wasm module with gas metering code.

pub use gear_wasm_instrument::gas_metering::*;
pub use rules::*;
pub use schedule::*;

mod rules;
mod schedule;
