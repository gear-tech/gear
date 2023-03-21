// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! `GlobalsAccessor` realizations for native and wasm runtimes.

use crate::common::{Error, GlobalNames};
use core::any::Any;
use gear_backend_common::lazy_pages::{GlobalsAccessError, GlobalsAccessMod, GlobalsAccessor};
use gear_core::memory::HostPointer;
use sc_executor_common::sandbox::SandboxInstance;
use sp_wasm_interface::Value;

#[derive(Debug, Clone, Copy)]
pub(crate) enum GlobalNo {
    GasLimit = 0,
    AllowanceLimit = 1,
    Amount = 2,
}

#[derive(Debug)]
pub(crate) struct GlobalsContext {
    /// Global names
    pub names: GlobalNames,
    /// Raw pointer to the globals access provider
    pub access_ptr: HostPointer,
    /// Access mod, currently two: native or WASM runtime
    pub access_mod: GlobalsAccessMod,
}

struct GlobalsAccessWasmRuntime<'a> {
    pub instance: &'a mut SandboxInstance,
}

impl<'a> GlobalsAccessor for GlobalsAccessWasmRuntime<'a> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.instance
            .get_global_val(name)
            .and_then(|value| match value {
                Value::I64(value) => Some(value),
                _ => None,
            })
            .ok_or(GlobalsAccessError)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.instance
            .set_global_val(name, Value::I64(value))
            .ok()
            .flatten()
            .ok_or(GlobalsAccessError)?;
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        unimplemented!("Has no use cases for this struct")
    }
}

struct GlobalsAccessNativeRuntime<'a, 'b> {
    pub inner_access_provider: &'a mut &'b mut dyn GlobalsAccessor,
}

impl<'a, 'b> GlobalsAccessor for GlobalsAccessNativeRuntime<'a, 'b> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.inner_access_provider.get_i64(name)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.inner_access_provider.set_i64(name, value)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        unimplemented!("Has no use cases for this struct")
    }
}

fn apply_for_global_internal(
    mut globals_access_provider: impl GlobalsAccessor,
    name: &str,
    mut f: impl FnMut(u64) -> Result<Option<u64>, Error>,
) -> Result<u64, Error> {
    let current_value = globals_access_provider.get_i64(name)? as u64;
    if let Some(new_value) = f(current_value)? {
        globals_access_provider.set_i64(name, new_value as i64)?;
        Ok(new_value)
    } else {
        Ok(current_value)
    }
}

pub(crate) unsafe fn apply_for_global(
    globals_ctx: &GlobalsContext,
    global_no: GlobalNo,
    f: impl FnMut(u64) -> Result<Option<u64>, Error>,
) -> Result<u64, Error> {
    let name = globals_ctx.names[global_no as usize].as_str();
    match globals_ctx.access_mod {
        GlobalsAccessMod::WasmRuntime => {
            let instance = (globals_ctx.access_ptr as *mut SandboxInstance)
                .as_mut()
                .ok_or(Error::HostInstancePointerIsInvalid)?;
            apply_for_global_internal(GlobalsAccessWasmRuntime { instance }, name, f)
        }
        GlobalsAccessMod::NativeRuntime => {
            let inner_access_provider = (globals_ctx.access_ptr as *mut &mut dyn GlobalsAccessor)
                .as_mut()
                .ok_or(Error::DynGlobalsAccessPointerIsInvalid)?;
            apply_for_global_internal(
                GlobalsAccessNativeRuntime {
                    inner_access_provider,
                },
                name,
                f,
            )
        }
    }
}
