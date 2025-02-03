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

//! Provide sp-sandbox support.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod env;
pub mod error;
mod funcs;
mod log;
pub mod memory;
#[cfg(any(feature = "mock", test))]
pub mod mock;
mod runtime;
pub mod state;

use gear_core::{
    env::Externalities,
    gas::{CountersOwner, GasAmount},
    memory::MemoryInterval,
};
use gear_lazy_pages_common::ProcessAccessError;

/// Extended externalities that can manage gas counters.
pub trait BackendExternalities: Externalities + CountersOwner {
    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if needed.
    fn pre_process_memory_accesses(
        &mut self,
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError>;
}

#[cfg(test)]
mod tests {
    use crate::{
        env::{BackendReport, Environment},
        error::ActorTerminationReason,
        mock::MockExt,
    };
    use gear_core::{gas_metering::CustomConstantCostRules, message::DispatchKind};
    use gear_wasm_instrument::{
        parity_wasm::{self, builder},
        InstrumentationBuilder, SyscallName,
    };

    /// Check that all syscalls are supported by backend.
    #[test]
    fn test_syscalls_table() {
        // Make module with one empty function.
        let mut module = builder::module()
            .function()
            .signature()
            .build()
            .build()
            .build();

        // Insert syscalls imports.
        for name in SyscallName::instrumentable() {
            let sign = name.signature();
            let types = module.type_section_mut().unwrap().types_mut();
            let type_no = types.len() as u32;
            types.push(parity_wasm::elements::Type::Function(sign.func_type()));

            module = builder::from_module(module)
                .import()
                .module("env")
                .external()
                .func(type_no)
                .field(name.to_str())
                .build()
                .build();
        }

        let module = InstrumentationBuilder::new("env")
            .with_gas_limiter(|_| CustomConstantCostRules::default())
            .instrument(module)
            .unwrap();
        let code = module.into_bytes().unwrap();

        // Execute wasm and check success.
        let ext = MockExt::default();
        let mut env =
            Environment::new(ext, &code, DispatchKind::Init, Default::default(), 0.into()).unwrap();
        let report = env.execute(|_, _, _| {}).unwrap();

        let BackendReport {
            termination_reason, ..
        } = report;

        assert_eq!(termination_reason, ActorTerminationReason::Success.into());
    }
}
