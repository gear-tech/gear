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

use std::fmt;

use anyhow::{Context as _, Result};
use arbitrary::{Arbitrary, Unstructured};
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

pub struct GeneratedModule<'u> {
    u: Unstructured<'u>,
    module: Module,
}

impl<'u> Arbitrary<'u> for GeneratedModule<'u> {
    fn arbitrary(u: &mut Unstructured<'u>) -> arbitrary::Result<Self> {
        let mut u = Unstructured::new(
            u.peek_bytes(u.len())
                .ok_or(arbitrary::Error::NotEnoughData)?,
        );

        Ok(GeneratedModule {
            module: generate_gear_program_module(&mut u, FuzzerConfigBundle)?,
            u,
        })
    }
}

impl GeneratedModule<'_> {
    pub fn enhance(self) -> Result<Self> {
        let module = self.module;

        let module = InstrumentationBuilder::new(MODULE_ENV)
            .with_gas_limiter(|_| FuzzerCostRules)
            .instrument(module)
            .map_err(anyhow::Error::msg)?;

        let (module, u) = InjectMemoryAccesses::new(self.u)
            .inject(module)
            .context("injected memory accesses")?;

        let (module, u) = InjectGlobals::new(u, Default::default())
            .inject(module)
            .context("injected globals")?;

        Ok(GeneratedModule { u, module })
    }

    pub fn module(self) -> Module {
        self.module
    }
}

impl fmt::Debug for GeneratedModule<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let module_str = wasmprinter::print_bytes(
            self.module
                .clone()
                .into_bytes()
                .expect("failed to serialize"),
        )
        .expect("failed to print module");

        write!(f, "{}", module_str)
    }
}
