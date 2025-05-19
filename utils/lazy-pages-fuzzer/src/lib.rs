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

use anyhow::{Context, Result};
use gear_wasm_instrument::Module;
use std::collections::BTreeMap;

mod config;

pub use generate::GeneratedModule;
mod generate;

mod globals;

use lazy_pages::{HostPageAddr, TouchedPage};
mod lazy_pages;

use wasmer_backend::WasmerRunner;
mod wasmer_backend;

use wasmi_backend::WasmiRunner;
mod wasmi_backend;

const INITIAL_PAGES: u32 = 10;
const PROGRAM_GAS: i64 = 1_000_000;
const OS_PAGE_SIZE: usize = 4096;
const MODULE_ENV: &str = "env";

trait Runner {
    fn run(module: &Module) -> Result<RunResult>;
}

/// Runs all the fuzz testing internal machinery.
pub fn run(generated_module: GeneratedModule) -> Result<()> {
    let module = generated_module.enhance()?.module();

    let unwrap_error_chain = |res| {
        match res {
            Ok(res) => res,
            Err(e) => {
                // Print whole error chain with '#' formatter
                panic!("{e:#?}")
            }
        }
    };

    let wasmer_res = unwrap_error_chain(WasmerRunner::run(&module).context("wasmer"));
    let wasmi_res = unwrap_error_chain(WasmiRunner::run(&module).context("wasmi"));

    RunResult::verify_equality(wasmer_res, wasmi_res);

    Ok(())
}

struct RunResult {
    gas_global: i64,
    pages: BTreeMap<HostPageAddr, (TouchedPage, Vec<u8>)>,
    globals: BTreeMap<String, i64>,
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
            assert_eq!(
                wasmer_page_info, wasmi_page_info,
                "wasmer page mem 0x{wasmer_addr:X?} wasmi page mem 0x{wasmi_addr:X?}",
            );
            assert_eq!(
                wasmer_page_mem, wasmi_page_mem,
                "wasmer page mem 0x{wasmer_addr:X?} wasmi page mem 0x{wasmi_addr:X?} \
                with content: 0x{wasmer_page_mem:X?} 0x{wasmi_page_mem:X?}",
            );
        }

        assert_eq!(wasmer_res.globals, wasmi_res.globals);
    }
}
