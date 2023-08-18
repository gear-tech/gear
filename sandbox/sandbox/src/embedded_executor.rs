// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! An embedded WASM executor utilizing `wasmi`.

use crate::{
    AsContext, Error, GlobalsSetError, HostError, HostFuncType, ReturnValue, SandboxCaller,
    SandboxStore, Value, TARGET,
};
use alloc::string::String;
use gear_sandbox_env::GLOBAL_NAME_GAS;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, prelude::*};
use sp_wasm_interface::HostPointer;
use wasmi::{
    core::{Pages, Trap},
    Engine, ExternType, Linker, MemoryType, Module, StoreContext, StoreContextMut,
    Value as RuntimeValue,
};

/// [`AsContext`] extension.
pub trait AsContextExt: wasmi::AsContext + wasmi::AsContextMut {}

/// wasmi store wrapper.
pub struct Store<T>(wasmi::Store<T>);

impl<T> Store<T> {
    fn engine(&self) -> &Engine {
        self.0.engine()
    }
}

impl<T> SandboxStore<T> for Store<T> {
    fn new(state: T) -> Self {
        let engine = Engine::default();
        let store = wasmi::Store::new(&engine, state);
        Self(store)
    }
}

impl<T> wasmi::AsContext for Store<T> {
    type UserState = T;

    fn as_context(&self) -> StoreContext<Self::UserState> {
        self.0.as_context()
    }
}

impl<T> wasmi::AsContextMut for Store<T> {
    fn as_context_mut(&mut self) -> StoreContextMut<Self::UserState> {
        self.0.as_context_mut()
    }
}

impl<T> AsContext<T> for Store<T> {
    fn data_mut(&mut self) -> &mut T {
        self.0.data_mut()
    }
}

impl<T> AsContextExt for Store<T> {}

/// wasmi caller wrapper.
pub struct Caller<'a, T>(wasmi::Caller<'a, T>);

impl<T> SandboxCaller<T> for Caller<'_, T> {
    fn set_global_val(&mut self, name: &str, value: Value) -> Option<()> {
        let global = self.0.get_export(name)?.into_global()?;
        global.set(&mut self.0, to_wasmi(value)).ok()?;
        Some(())
    }

    fn get_global_val(&self, name: &str) -> Option<Value> {
        let value = self.0.get_export(name)?.into_global()?.get(&self.0);
        Some(to_interface(value))
    }
}

impl<T> wasmi::AsContext for Caller<'_, T> {
    type UserState = T;

    fn as_context(&self) -> StoreContext<Self::UserState> {
        self.0.as_context()
    }
}

impl<T> wasmi::AsContextMut for Caller<'_, T> {
    fn as_context_mut(&mut self) -> StoreContextMut<Self::UserState> {
        self.0.as_context_mut()
    }
}

impl<T> AsContext<T> for Caller<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.0.data_mut()
    }
}

impl<T> AsContextExt for Caller<'_, T> {}

/// The linear memory used by the sandbox.
#[derive(Clone)]
pub struct Memory {
    memref: wasmi::Memory,
}

impl<T> super::SandboxMemory<T> for Memory {
    fn new(store: &mut Store<T>, initial: u32, maximum: Option<u32>) -> Result<Memory, Error> {
        let ty = MemoryType::new(initial, maximum).map_err(|_| Error::Module)?;
        let memref = wasmi::Memory::new(store, ty).map_err(|_| Error::Module)?;
        Ok(Memory { memref })
    }

    fn get<C>(&self, ctx: &C, ptr: u32, buf: &mut [u8]) -> Result<(), Error>
    where
        C: AsContext<T>,
    {
        let data = self
            .memref
            .data(ctx)
            .get((ptr as usize)..(ptr as usize + buf.len()))
            .ok_or(Error::OutOfBounds)?;
        buf[..].copy_from_slice(data);
        Ok(())
    }

    fn set<C>(&self, ctx: &mut C, ptr: u32, value: &[u8]) -> Result<(), Error>
    where
        C: AsContext<T>,
    {
        let data = self
            .memref
            .data_mut(ctx)
            .get_mut((ptr as usize)..(ptr as usize + value.len()))
            .ok_or(Error::OutOfBounds)?;
        data[..].copy_from_slice(value);
        Ok(())
    }

    fn grow<C>(&self, ctx: &mut C, pages: u32) -> Result<u32, Error>
    where
        C: AsContext<T>,
    {
        let pages = Pages::new(pages).ok_or(Error::MemoryGrow)?;
        self.memref
            .grow(ctx, pages)
            .map(Into::into)
            .map_err(|_| Error::MemoryGrow)
    }

    fn size<C>(&self, ctx: &C) -> u32
    where
        C: AsContext<T>,
    {
        self.memref.current_pages(ctx).into()
    }

    unsafe fn get_buff<C>(&self, ctx: &mut C) -> u64
    where
        C: AsContext<T>,
    {
        self.memref.data_mut(ctx).as_mut_ptr() as usize as u64
    }
}

enum ExternVal<T> {
    HostFunc(HostFuncType<T>),
    Memory(Memory),
}

impl<T> ExternVal<T> {
    fn host_func(self) -> Option<HostFuncType<T>> {
        match self {
            ExternVal::HostFunc(ptr) => Some(ptr),
            ExternVal::Memory(_) => None,
        }
    }

    fn memory(self) -> Option<Memory> {
        match self {
            ExternVal::HostFunc(_) => None,
            ExternVal::Memory(mem) => Some(mem),
        }
    }
}

impl<T> Clone for ExternVal<T> {
    fn clone(&self) -> Self {
        match self {
            ExternVal::HostFunc(func) => ExternVal::HostFunc(*func),
            ExternVal::Memory(mem) => ExternVal::Memory(mem.clone()),
        }
    }
}

/// A builder for the environment of the sandboxed WASM module.
pub struct EnvironmentDefinitionBuilder<T> {
    map: BTreeMap<(String, String), ExternVal<T>>,
}

impl<T> super::SandboxEnvironmentBuilder<T, Memory> for EnvironmentDefinitionBuilder<T> {
    fn new() -> Self {
        EnvironmentDefinitionBuilder {
            map: BTreeMap::new(),
        }
    }

    fn add_host_func<N1, N2>(&mut self, module: N1, field: N2, f: HostFuncType<T>)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        self.map
            .insert((module.into(), field.into()), ExternVal::HostFunc(f));
    }

    fn add_memory<N1, N2>(&mut self, module: N1, field: N2, mem: Memory)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        self.map
            .insert((module.into(), field.into()), ExternVal::Memory(mem));
    }
}

/// Sandboxed instance of a WASM module.
pub struct Instance<State> {
    instance: wasmi::Instance,
    _marker: PhantomData<State>,
}

impl<State> Clone for Instance<State> {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance,
            _marker: PhantomData,
        }
    }
}

impl<State: 'static> super::SandboxInstance<State> for Instance<State> {
    type Memory = Memory;
    type EnvironmentBuilder = EnvironmentDefinitionBuilder<State>;

    fn new(
        mut store: &mut Store<State>,
        code: &[u8],
        env_def_builder: &Self::EnvironmentBuilder,
    ) -> Result<Instance<State>, Error> {
        let module = Module::new(store.engine(), code).map_err(|_| Error::Module)?;
        let mut linker = Linker::new(store.engine());

        for import in module.imports() {
            let module = import.module().to_string();
            let name = import.name().to_string();
            let key = (module.clone(), name.clone());

            match import.ty() {
                ExternType::Global(_) | ExternType::Table(_) => {}
                ExternType::Memory(mem_ty) => {
                    let mem = env_def_builder
                        .map
                        .get(&key)
                        .cloned()
                        .and_then(|val| val.memory())
                        .ok_or(Error::Module)?
                        .memref;

                    if mem.ty(&store) != *mem_ty {
                        return Err(Error::Module);
                    }

                    let mem = wasmi::Extern::Memory(mem);
                    linker
                        .define(&module, &name, mem)
                        .map_err(|_| Error::Module)?;
                }
                ExternType::Func(func_ty) => {
                    let func_ptr = env_def_builder
                        .map
                        .get(&key)
                        .cloned()
                        .and_then(|val| val.host_func())
                        .ok_or(Error::Module)?;

                    let func = wasmi::Func::new(
                        &mut store,
                        func_ty.clone(),
                        move |caller, params, results| {
                            let caller = Caller(caller);

                            let gas = caller.get_global_val(GLOBAL_NAME_GAS).ok_or_else(|| {
                                Trap::new(format!("failed to get `{GLOBAL_NAME_GAS}` global"))
                            })?;
                            let params: Vec<_> = Some(gas)
                                .into_iter()
                                .chain(params.iter().cloned().map(to_interface))
                                .collect();

                            let val = (func_ptr)(caller, &params)
                                .map_err(|HostError| Trap::new("function error"))?;

                            match (val.inner, results) {
                                (ReturnValue::Unit, []) => {}
                                (ReturnValue::Value(val), [ret]) => {
                                    let val = to_wasmi(val);

                                    if val.ty() != ret.ty() {
                                        return Err(Trap::new("mismatching return types"));
                                    }

                                    *ret = val;
                                }
                                _ => unreachable!(),
                            }

                            Ok(())
                        },
                    );
                    let func = wasmi::Extern::Func(func);
                    linker
                        .define(&module, &name, func)
                        .map_err(|_| Error::Module)?;
                }
            }
        }

        let instance_pre = linker.instantiate(&mut store, &module).map_err(|e| {
            log::error!(target: TARGET, "Error instantiating module: {:?}", e);
            Error::Module
        })?;
        let instance = instance_pre.start(&mut store).map_err(|e| {
            log::error!(target: TARGET, "Error starting module: {:?}", e);
            Error::Module
        })?;

        Ok(Instance {
            instance,
            _marker: PhantomData,
        })
    }

    fn invoke(
        &mut self,
        mut store: &mut Store<State>,
        name: &str,
        args: &[Value],
    ) -> Result<ReturnValue, Error> {
        let args = args.iter().cloned().map(to_wasmi).collect::<Vec<_>>();

        let func = self
            .instance
            .get_func(&store, name)
            .ok_or(Error::Execution)?;

        let func_ty = func.ty(&store);
        let mut results =
            vec![RuntimeValue::ExternRef(wasmi::ExternRef::null()); func_ty.results().len()];

        func.call(&mut store, &args, &mut results)
            .map_err(|_| Error::Execution)?;

        match results.as_slice() {
            [] => Ok(ReturnValue::Unit),
            [val] => Ok(ReturnValue::Value(to_interface(val.clone()))),
            _ => unreachable!(),
        }
    }

    fn get_global_val(&self, store: &Store<State>, name: &str) -> Option<Value> {
        let global = self.instance.get_global(store, name)?;
        let global = global.get(store);
        Some(to_interface(global))
    }

    fn set_global_val(
        &self,
        mut store: &mut Store<State>,
        name: &str,
        value: Value,
    ) -> Result<(), GlobalsSetError> {
        let global = self
            .instance
            .get_global(&store, name)
            .ok_or(GlobalsSetError::NotFound)?;
        global
            .set(&mut store, to_wasmi(value))
            .map_err(|_| GlobalsSetError::Other)?;
        Ok(())
    }

    fn get_instance_ptr(&self) -> HostPointer {
        unreachable!("Must not be called for embedded executor")
    }
}

/// Convert the substrate value type to the wasmi value type.
fn to_wasmi(value: Value) -> RuntimeValue {
    match value {
        Value::I32(val) => RuntimeValue::I32(val),
        Value::I64(val) => RuntimeValue::I64(val),
        Value::F32(val) => RuntimeValue::F32(val.into()),
        Value::F64(val) => RuntimeValue::F64(val.into()),
    }
}

/// Convert the wasmi value type to the substrate value type.
fn to_interface(value: RuntimeValue) -> Value {
    match value {
        RuntimeValue::I32(val) => Value::I32(val),
        RuntimeValue::I64(val) => Value::I64(val),
        RuntimeValue::F32(val) => Value::F32(val.into()),
        RuntimeValue::F64(val) => Value::F64(val.into()),
        RuntimeValue::FuncRef(_) | RuntimeValue::ExternRef(_) => {
            unreachable!("embedded executor doesn't work with FuncRef or ExternRef")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Caller, EnvironmentDefinitionBuilder, Instance};
    use crate::{
        default_executor::Store, AsContext, Error, HostError, ReturnValue,
        SandboxEnvironmentBuilder, SandboxInstance, SandboxStore, Value,
    };
    use assert_matches::assert_matches;
    use gear_sandbox_env::WasmReturnValue;

    fn execute_sandboxed(code: &[u8], args: &[Value]) -> Result<ReturnValue, Error> {
        struct State {
            counter: u32,
        }

        fn env_assert(_c: Caller<'_, State>, args: &[Value]) -> Result<WasmReturnValue, HostError> {
            if args.len() != 1 {
                return Err(HostError);
            }
            let condition = args[0].as_i32().ok_or(HostError)?;
            if condition != 0 {
                Ok(WasmReturnValue {
                    gas: 0,
                    inner: ReturnValue::Unit,
                })
            } else {
                Err(HostError)
            }
        }
        fn env_inc_counter(
            mut e: Caller<'_, State>,
            args: &[Value],
        ) -> Result<WasmReturnValue, HostError> {
            let e = e.data_mut();
            if args.len() != 1 {
                return Err(HostError);
            }
            let inc_by = args[0].as_i32().ok_or(HostError)?;
            e.counter += inc_by as u32;
            Ok(WasmReturnValue {
                gas: 0,
                inner: ReturnValue::Value(Value::I32(e.counter as i32)),
            })
        }
        /// Function that takes one argument of any type and returns that value.
        fn env_polymorphic_id(
            _c: Caller<'_, State>,
            args: &[Value],
        ) -> Result<WasmReturnValue, HostError> {
            if args.len() != 1 {
                return Err(HostError);
            }
            Ok(WasmReturnValue {
                gas: 0,
                inner: ReturnValue::Value(args[0]),
            })
        }

        let state = State { counter: 0 };

        let mut env_builder = EnvironmentDefinitionBuilder::new();
        env_builder.add_host_func("env", "assert", env_assert);
        env_builder.add_host_func("env", "inc_counter", env_inc_counter);
        env_builder.add_host_func("env", "polymorphic_id", env_polymorphic_id);

        let mut store = Store::new(state);
        let mut instance = Instance::new(&mut store, code, &env_builder)?;
        instance.invoke(&mut store, "call", args)
    }

    #[test]
    fn invoke_args() {
        let code = wat::parse_str(
            r#"
		(module
			(import "env" "assert" (func $assert (param i32)))

			(func (export "call") (param $x i32) (param $y i64)
				;; assert that $x = 0x12345678
				(call $assert
					(i32.eq
						(get_local $x)
						(i32.const 0x12345678)
					)
				)

				(call $assert
					(i64.eq
						(get_local $y)
						(i64.const 0x1234567887654321)
					)
				)
			)
		)
		"#,
        )
        .unwrap();

        execute_sandboxed(
            &code,
            &[Value::I32(0x12345678), Value::I64(0x1234567887654321)],
        )
        .unwrap();
    }

    #[test]
    fn return_value() {
        let code = wat::parse_str(
            r#"
		(module
			(func (export "call") (param $x i32) (result i32)
				(i32.add
					(get_local $x)
					(i32.const 1)
				)
			)
		)
		"#,
        )
        .unwrap();

        let return_val = execute_sandboxed(&code, &[Value::I32(0x1336)]).unwrap();
        assert_eq!(return_val, ReturnValue::Value(Value::I32(0x1337)));
    }

    #[test]
    fn cant_return_unmatching_type() {
        fn env_returns_i32(
            _e: Caller<'_, ()>,
            _args: &[Value],
        ) -> Result<WasmReturnValue, HostError> {
            Ok(WasmReturnValue {
                gas: 0,
                inner: ReturnValue::Value(Value::I32(42)),
            })
        }

        let mut env_builder = EnvironmentDefinitionBuilder::new();
        env_builder.add_host_func("env", "returns_i32", env_returns_i32);

        let code = wat::parse_str(
            r#"
		(module
			;; It's actually returns i32, but imported as if it returned i64
			(import "env" "returns_i32" (func $returns_i32 (result i64)))

			(func (export "call")
				(drop
					(call $returns_i32)
				)
			)
		)
		"#,
        )
        .unwrap();

        let mut store = Store::new(());
        // It succeeds since we are able to import functions with types we want.
        let mut instance = Instance::new(&mut store, &code, &env_builder).unwrap();

        // But this fails since we imported a function that returns i32 as if it returned i64.
        assert_matches!(
            instance.invoke(&mut store, "call", &[]),
            Err(Error::Execution)
        );
    }
}
