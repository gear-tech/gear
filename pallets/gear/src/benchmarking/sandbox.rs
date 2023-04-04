// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

/// ! For instruction benchmarking we do no instantiate a full program but merely the
/// ! sandbox to execute the wasm code. This is because we do not need the full
/// ! environment that provides the seal interface as imported functions.
use super::{
    code::{ModuleDefinition, WasmModule},
    Config,
};

use common::Origin;
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory},
    SandboxEnvironmentBuilder, SandboxInstance,
};

/// Minimal execution environment without any exported functions.
pub struct Sandbox {
    instance: Instance<()>,
    _memory: Option<Memory>,
}

impl Sandbox {
    /// Invoke the `handle` function of a program code and panic on any execution error.
    pub fn invoke(&mut self) {
        self.instance.invoke("handle", &[], &mut ()).unwrap();
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
        let memory = module.add_memory(&mut env_builder);
        let instance = Instance::new(&module.code, &env_builder, &mut ())
            .expect("Failed to create benchmarking Sandbox instance");
        Self {
            instance,
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
        let memory = module.add_memory(&mut env_builder);
        let instance = Instance::new(&module.code, &env_builder, &mut ())
            .expect("Failed to create benchmarking Sandbox instance");
        Self {
            instance,
            _memory: memory,
        }
    }
}
