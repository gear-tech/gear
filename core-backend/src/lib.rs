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

//! Provide sp-sandbox support.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod accessors;
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
    memory::{Memory, MemoryDump, MemoryError, MemoryInterval},
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

pub trait MemoryStorer {
    fn dump_memory<Context>(
        &mut self,
        ctx: &Context,
        memory: &impl Memory<Context>,
    ) -> Result<MemoryDump, MemoryError>;

    fn revert_memory<Context>(
        &self,
        ctx: &mut Context,
        memory: &impl Memory<Context>,
    ) -> Result<(), MemoryError>;
}

pub struct DummyStorer;
impl MemoryStorer for DummyStorer {
    fn dump_memory<Context>(
        &mut self,
        _ctx: &Context,
        _memory: &impl Memory<Context>,
    ) -> Result<MemoryDump, MemoryError> {
        Err(Default::default())
    }

    fn revert_memory<Context>(
        &self,
        _ctx: &mut Context,
        _memory: &impl Memory<Context>,
    ) -> Result<(), MemoryError> {
        Err(Default::default())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        DummyStorer,
        env::{BackendReport, Environment},
        error::ActorTerminationReason,
        mock::MockExt,
    };
    use gear_core::{gas_metering::CustomConstantCostRules, message::DispatchKind};
    use gear_wasm_instrument::{
        FuncType, Function, Import, InstrumentationBuilder, ModuleBuilder, SyscallName,
    };
    use tracing_subscriber::EnvFilter;

    /// Check that all syscalls are supported by backend.
    #[test]
    fn test_syscalls_table() {
        tracing_subscriber::fmt::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_test_writer()
            .init();

        // Make module with one empty function.
        let mut module = ModuleBuilder::default();
        module.add_func(FuncType::new([], []), Function::default());

        // Insert syscalls imports.
        for name in SyscallName::instrumentable() {
            let sign = name.signature();
            let type_no = module.push_type(sign.func_type());

            module.push_import(Import::func("env", name.to_str(), type_no));
        }

        let module = InstrumentationBuilder::new("env")
            .with_gas_limiter(|_| CustomConstantCostRules::default())
            .instrument(module.build())
            .unwrap();
        let code = module.serialize().unwrap();

        // Execute wasm and check success.
        let ext = MockExt::default();
        let env = Environment::new(ext, &code, Default::default(), 0.into(), |_, _, _| {}).unwrap();
        let execution_result = env
            .execute(DispatchKind::Init, None::<&mut DummyStorer>)
            .unwrap();

        let BackendReport {
            termination_reason, ..
        } = execution_result.expect("Expecting success run").report();

        assert_eq!(termination_reason, ActorTerminationReason::Success.into());
    }
}
