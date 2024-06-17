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

use anyhow::{Context, Result};

use arbitrary::Unstructured;
use gear_wasm_gen::generate_gear_program_module;
use gear_wasm_instrument::{parity_wasm::elements::Module, InstrumentationBuilder};

use crate::{
    config::{FuzzerConfigBundle, FuzzerCostRules},
    MODULE_ENV,
};

use globals::InjectGlobals;
pub use globals::GLOBAL_NAME_PREFIX;
mod globals;

use mem_accesses::InjectMemoryAccesses;
mod mem_accesses;

pub fn generate_module(mut u: Unstructured<'_>) -> Result<Module> {
    let module =
        generate_gear_program_module(&mut u, FuzzerConfigBundle).context("module generated")?;

    let module = InstrumentationBuilder::new(MODULE_ENV)
        .with_gas_limiter(|_| FuzzerCostRules)
        .instrument(module)
        .map_err(anyhow::Error::msg)?;

    let (module, u) = InjectMemoryAccesses::new(u)
        .inject(module)
        .context("injected memory accesses")?;

    let (module, _) = InjectGlobals::new(u, Default::default())
        .inject(module)
        .context("injected globals")?;

    Ok(module)
}
