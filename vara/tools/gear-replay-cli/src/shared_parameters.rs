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

use clap::Parser;
use sc_cli::{
    DEFAULT_WASM_EXECUTION_METHOD, DEFAULT_WASMTIME_INSTANTIATION_STRATEGY, WasmExecutionMethod,
    WasmtimeInstantiationStrategy,
};
use std::fmt::Debug;

/// Parameters shared across the subcommands
#[derive(Clone, Debug, Parser)]
#[group(skip)]
pub struct SharedParams {
    /// Type of wasm execution used.
    #[arg(
		long = "wasm-execution",
		value_name = "METHOD",
		value_enum,
		ignore_case = true,
		default_value_t = DEFAULT_WASM_EXECUTION_METHOD,
	)]
    pub wasm_method: WasmExecutionMethod,

    /// The WASM instantiation method to use.
    ///
    /// Only has an effect when `wasm-execution` is set to `compiled`.
    #[arg(
		long = "wasm-instantiation-strategy",
		value_name = "STRATEGY",
		default_value_t = DEFAULT_WASMTIME_INSTANTIATION_STRATEGY,
		value_enum,
	)]
    pub wasmtime_instantiation_strategy: WasmtimeInstantiationStrategy,

    /// The number of 64KB pages to allocate for Wasm execution. Defaults to
    /// [`sc_service::Configuration.default_heap_pages`].
    #[arg(long)]
    pub heap_pages: Option<u64>,

    /// Sets a custom logging filter. Syntax is `<target>=<level>`, e.g. -lsync=debug.
    ///
    /// Log levels (least to most verbose) are error, warn, info, debug, and trace.
    /// By default, all targets log `info`. The global log level can be set with `-l<level>`.
    #[arg(short = 'l', long, value_name = "NODE_LOG", num_args = 0..)]
    pub log: Vec<String>,
}
