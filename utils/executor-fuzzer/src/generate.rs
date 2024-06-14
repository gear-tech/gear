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
use gear_wasm_gen::generate_gear_program_module;
use gear_wasm_instrument::{parity_wasm::elements::Module, InstrumentationBuilder};
use mem_accesses::InjectMemoryAccesses;

use crate::{
    config::{FuzzerConfigBundle, FuzzerCostRules},
    ENV,
};

mod globals;
mod mem_accesses;

pub fn generate_module(mut u: Unstructured<'_>) -> Result<Module> {
    let module =
        generate_gear_program_module(&mut u, FuzzerConfigBundle).expect("module generated");

    let module = InstrumentationBuilder::new(ENV)
        .with_gas_limiter(|_| FuzzerCostRules)
        .instrument(module)
        .expect("instrumented");

    let module = InjectMemoryAccesses::new(u, module)
        .inject()
        .expect("injected memory accesses");

    Ok(module)
}
