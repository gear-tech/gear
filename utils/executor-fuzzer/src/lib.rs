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

use std::collections::BTreeMap;

use anyhow::Result;

use arbitrary::Unstructured;
pub use config::FuzzerInput;
use gear_wasm_instrument::parity_wasm::elements::Module;
use lazy_pages::{HostPageAddr, TouchedPage};
use wasmer_backend::WasmerRunner;
use wasmi_backend::WasmiRunner;

mod config;
mod generate;
mod globals;
mod lazy_pages;
mod wasmer_backend;
mod wasmi_backend;

const INITIAL_PAGES: u32 = 10;
const WASM_PAGE_SIZE: usize = 0x10_000;
const PROGRAM_GAS: i64 = 10_000_000;
const ENV: &str = "env";

trait Runner {
    fn run(module: &Module) -> Result<RunResult>;
}

/// Runs all the fuzz testing internal machinery.
pub fn run(data: FuzzerInput) -> Result<()> {
    let module = generate::generate_module(Unstructured::new(data.0))?;

    print_module(&module);

    let wasmer_res = WasmerRunner::run(&module)?;
    let wasmi_res = WasmiRunner::run(&module)?;

    RunResult::verify_equality(wasmer_res, wasmi_res);

    Ok(())
}

fn print_module(m: &Module) {
    let b = m.clone().into_bytes().unwrap();
    println!(
        "{}",
        wasmprinter::print_bytes(b).expect("failed to print module")
    );
}

struct RunResult {
    gas_global: i64,
    // TODO: globals
    pages: BTreeMap<HostPageAddr, (TouchedPage, Vec<u8>)>,
}

impl RunResult {
    fn verify_equality(wasmer_res: Self, wasmi_res: Self) {
        assert_eq!(wasmer_res.gas_global, wasmi_res.gas_global);
        assert_eq!(wasmer_res.pages.len(), wasmi_res.pages.len());

        for (
            (wasmer_addr, (wasmer_page_info, wasmer_page_mem)),
            (wasmi_addr, (wasmi_page_info, wasmi_page_mem)),
        ) in wasmer_res
            .pages
            .into_iter()
            .zip(wasmi_res.pages.into_iter())
        {
            let lower_bytes_page_mask = ((INITIAL_PAGES as usize) * WASM_PAGE_SIZE) - 1;
            assert_eq!(
                lower_bytes_page_mask & wasmer_addr,
                lower_bytes_page_mask & wasmi_addr
            );
            assert_eq!(wasmer_page_info, wasmi_page_info);
            assert_eq!(wasmer_page_mem, wasmi_page_mem);
        }
    }
}
