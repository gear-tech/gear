// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
        FuncType, Function, Import, InstrumentationBuilder, ModuleBuilder, SyscallKind, SyscallName,
    };
    use tracing_subscriber::EnvFilter;

    /// Check that all syscalls are supported by backend.
    #[test]
    fn test_syscalls_table() {
        tracing_subscriber::fmt::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_test_writer()
            .init();

        for syscall_kind in [SyscallKind::Vara, SyscallKind::Eth] {
            // Make module with one empty function.
            let mut module = ModuleBuilder::default();
            module.add_func(FuncType::new([], []), Function::default());

            // Insert syscalls imports.
            for name in SyscallName::instrumentable(syscall_kind) {
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
            let env = Environment::new(
                ext,
                &code,
                DispatchKind::Init,
                Default::default(),
                0.into(),
                syscall_kind,
            )
            .unwrap();
            let report = env.execute(|_, _, _| {}).unwrap();

            let BackendReport {
                termination_reason, ..
            } = report;

            assert_eq!(termination_reason, ActorTerminationReason::Success.into());
        }
    }
}
