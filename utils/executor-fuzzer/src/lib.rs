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

use anyhow::Result;

use arbitrary::Unstructured;
pub use config::FuzzerInput;
use gear_wasm_instrument::parity_wasm::elements::Module;
use wasmer_backend::WasmerRunner;
use wasmi_backend::WasmiRunner;

mod config;
mod generate;
mod globals;
mod lazy_pages;
mod wasmer_backend;
mod wasmi_backend;

const MEMORY_BYTES: u32 = 0x10_000;
const PROGRAM_GAS: i64 = 10_000_000;
const ENV: &str = "env";

trait Runner {
    fn run(module: &Module) -> Result<()>;
}

/// Runs all the fuzz testing internal machinery.
pub fn run(data: FuzzerInput) -> Result<()> {
    let module = generate::generate_module(Unstructured::new(data.0))?;

    WasmerRunner::run(&module)?;
    WasmiRunner::run(&module)?;

    // TODO: compare the results of both runners

    Ok(())
}

fn print_module(m: &Module) {
    let b = m.clone().into_bytes().unwrap();
    println!(
        "{}",
        wasmprinter::print_bytes(b).expect("failed to print module")
    );
}
