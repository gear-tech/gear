// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/// ! For instruction benchmarking we do no instantiate a full program but merely the
/// ! sandbox to execute the wasm code. This is because we do not need the full
/// ! environment that provides the seal interface as imported functions.
use super::{
    Config,
    code::{ModuleDefinition, WasmModule},
};

use common::Origin;
use gear_sandbox::{
    SandboxEnvironmentBuilder, SandboxInstance, SandboxStore,
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory, Store},
};

/// Minimal execution environment without any exported functions.
pub struct Sandbox {
    instance: Instance<()>,
    store: Store<()>,
    _memory: Option<Memory>,
}

impl Sandbox {
    /// Invoke the `handle` function of a program code and panic on any execution error.
    pub fn invoke(&mut self) {
        self.instance
            .invoke(&mut self.store, "handle", &[])
            .unwrap();
    }
}

impl<T: Config> From<&WasmModule<T>> for Sandbox
where
    T: Config,
    T::AccountId: Origin,
{
    /// Creates an instance from the supplied module and supplies as much memory
    /// to the instance as the module declares as imported.
    fn from(module: &WasmModule<T>) -> Self {
        let mut env_builder = EnvironmentDefinitionBuilder::new();
        let mut store = Store::new(());
        let memory = module.add_memory(&mut store, &mut env_builder);
        let instance = Instance::new(&mut store, &module.code, &env_builder)
            .expect("Failed to create benchmarking Sandbox instance");
        Self {
            instance,
            store,
            _memory: memory,
        }
    }
}

impl Sandbox {
    /// Creates an instance from the supplied module and supplies as much memory
    /// to the instance as the module declares as imported.
    pub fn from_module_def<T>(module: ModuleDefinition) -> Self
    where
        T: Config,
        T::AccountId: Origin,
    {
        let module: WasmModule<T> = module.into();
        let mut env_builder = EnvironmentDefinitionBuilder::new();
        let mut store = Store::new(());
        let memory = module.add_memory(&mut store, &mut env_builder);
        let instance = Instance::new(&mut store, &module.code, &env_builder)
            .expect("Failed to create benchmarking Sandbox instance");
        Self {
            instance,
            store,
            _memory: memory,
        }
    }
}
