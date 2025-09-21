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

//! An embedded WASM executor utilizing `wasmtime`.

use crate::{
    AsContextExt, Error, GlobalsSetError, HostError, HostFuncType, ReturnValue, SandboxStore, Value,
};
use alloc::string::String;
use anyhow::{Context, anyhow};
use gear_sandbox_env::GLOBAL_NAME_GAS;
use sp_wasm_interface_common::HostPointer;
use std::{
    collections::btree_map::BTreeMap, env, fs, marker::PhantomData, path::PathBuf, sync::OnceLock,
};
use wasmtime::{
    Cache, CacheConfig, Config, Engine, ExternType, Global, Linker, MemoryType, Module,
    StoreContext, StoreContextMut,
};

/// The target used for logging.
const TARGET: &str = "runtime::sandbox";

fn cache_base_path() -> PathBuf {
    static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
    CACHE_DIR
        .get_or_init(|| {
            // We acquire workspace root dir during runtime and compile-time.
            //
            // During development, runtime workspace dir equals to compile-time one,
            // so all compiled WASMs are cached in the usual ` OUT_DIR `
            // like we don't rewrite it.
            //
            // During cross-compilation, the runtime workspace dir differs from the compile-time one,
            // and accordingly, `OUT_DIR` beginning differs too,
            // so we change its beginning to successfully run tests.
            //
            // `OUT_DIR` is used for caching instead of some platform-specific project folder to
            // not maintain the ever-growing number of cached WASMs

            let out_dir = PathBuf::from(env!("OUT_DIR"));

            let runtime_workspace_dir = env::var_os("GEAR_WORKSPACE_DIR").map(PathBuf::from);
            let compiled_workspace_dir = option_env!("GEAR_WORKSPACE_DIR").map(PathBuf::from);
            let (Some(runtime_workspace_dir), Some(compiled_workspace_dir)) =
                (runtime_workspace_dir, compiled_workspace_dir)
            else {
                // `GEAR_WORKSPACE_DIR` is not present in user code,
                // so we return `OUT_DIR` without any changes
                return out_dir;
            };

            let out_dir = pathdiff::diff_paths(out_dir, compiled_workspace_dir).unwrap();
            let out_dir = runtime_workspace_dir.join(out_dir);

            let cache = out_dir.join("wasmtime-cache");
            fs::create_dir_all(&cache).unwrap();
            cache
        })
        .into()
}

/// [`AsContextExt`] extension.
pub trait AsContext: wasmtime::AsContextMut {}

#[derive(Debug)]
pub struct InnerState<T> {
    inner: T,
    gas_global: Option<Global>,
}

impl<T> InnerState<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            gas_global: None,
        }
    }
}

/// wasmtime store wrapper.
#[derive(Debug)]
pub struct Store<T: 'static> {
    inner: wasmtime::Store<InnerState<T>>,
}

impl<T> Store<T> {
    fn engine(&self) -> &Engine {
        self.inner.engine()
    }
}

impl<T: Send + 'static> SandboxStore for Store<T> {
    fn new(state: T) -> Self {
        let mut cache = CacheConfig::new();
        cache.with_directory(cache_base_path());
        // TODO: return, don't unwrap
        let cache = Cache::new(cache).expect("Failed to create cache memory");

        let mut config = Config::new();
        config
            .max_wasm_stack(16 * 1024 * 1024) // make stack size bigger for fuzzer
            .strategy(wasmtime::Strategy::Winch)
            .cache(Some(cache));
        // TODO: return, don't unwrap
        let engine = Engine::new(&config).expect("TODO");
        let store = wasmtime::Store::new(&engine, InnerState::new(state));

        Self { inner: store }
    }
}

impl<T> wasmtime::AsContext for Store<T> {
    type Data = InnerState<T>;

    fn as_context(&self) -> StoreContext<'_, Self::Data> {
        self.inner.as_context()
    }
}

impl<T> wasmtime::AsContextMut for Store<T> {
    fn as_context_mut(&mut self) -> StoreContextMut<'_, Self::Data> {
        self.inner.as_context_mut()
    }
}

impl<T: Send + 'static> AsContextExt for Store<T> {
    type State = T;

    fn data_mut(&mut self) -> &mut Self::State {
        &mut self.inner.data_mut().inner
    }
}

impl<T> AsContext for Store<T> {}

/// wasmtime function env wrapper.
pub struct Caller<'a, T: 'static>(wasmtime::Caller<'a, InnerState<T>>);

impl<T> wasmtime::AsContext for Caller<'_, T> {
    type Data = InnerState<T>;

    fn as_context(&self) -> StoreContext<'_, Self::Data> {
        self.0.as_context()
    }
}

impl<T> wasmtime::AsContextMut for Caller<'_, T> {
    fn as_context_mut(&mut self) -> StoreContextMut<'_, Self::Data> {
        self.0.as_context_mut()
    }
}

impl<T: Send + 'static> AsContextExt for Caller<'_, T> {
    type State = T;

    fn data_mut(&mut self) -> &mut Self::State {
        &mut self.0.data_mut().inner
    }
}

impl<T> AsContext for Caller<'_, T> {}

/// The linear memory used by the sandbox.
#[derive(Clone)]
pub struct Memory {
    memref: wasmtime::Memory,
    base: usize,
}

impl<T> super::SandboxMemory<T> for Memory {
    fn new(store: &mut Store<T>, initial: u32, maximum: Option<u32>) -> Result<Memory, Error> {
        let ty = MemoryType::new(initial, maximum);
        let memref = wasmtime::Memory::new(&mut *store, ty).map_err(|e| {
            log::trace!("Failed to create memory: {e}");
            Error::Module
        })?;
        let base = memref.data_ptr(&mut *store) as usize;
        Ok(Memory { memref, base })
    }

    fn read<Context>(&self, ctx: &Context, ptr: u32, buf: &mut [u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .read(ctx, ptr as usize, buf)
            .map_err(|_| Error::OutOfBounds)?;
        Ok(())
    }

    fn write<Context>(&self, ctx: &mut Context, ptr: u32, value: &[u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .write(ctx, ptr as usize, value)
            .map_err(|_| Error::OutOfBounds)?;
        Ok(())
    }

    fn grow<Context>(&self, ctx: &mut Context, pages: u32) -> Result<u32, Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .grow(ctx, pages as u64)
            .map(|pages| pages as u32)
            .map_err(|_| Error::MemoryGrow)
    }

    fn size<Context>(&self, ctx: &Context) -> u32
    where
        Context: AsContextExt<State = T>,
    {
        self.memref.size(ctx) as u32
    }

    unsafe fn get_buff<Context>(&self, _ctx: &Context) -> u64
    where
        Context: AsContextExt<State = T>,
    {
        self.base as u64
    }
}

enum ExternVal<T: 'static> {
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
pub struct EnvironmentDefinitionBuilder<T: 'static> {
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
    instance: wasmtime::Instance,
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

impl<State: Send + 'static> super::SandboxInstance<State> for Instance<State> {
    type Memory = Memory;
    type EnvironmentBuilder = EnvironmentDefinitionBuilder<State>;

    fn new(
        store: &mut Store<State>,
        code: &[u8],
        env_def_builder: &Self::EnvironmentBuilder,
    ) -> Result<Instance<State>, Error> {
        let module = Module::new(store.engine(), code)
            .inspect_err(|e| log::trace!(target: TARGET, "Failed to create module: {e}"))
            .map_err(|_e| Error::Module)?;

        let mut linker = Linker::new(store.engine());

        for import in module.imports() {
            let module = import.module().to_string();
            let name = import.name().to_string();
            let key = (module.clone(), name.clone());

            match import.ty() {
                ExternType::Global(_) | ExternType::Table(_) | ExternType::Tag(_) => {}
                ExternType::Memory(_mem_ty) => {
                    let mem = env_def_builder
                        .map
                        .get(&key)
                        .cloned()
                        .and_then(|val| val.memory())
                        .ok_or_else(|| {
                            log::trace!("Memory import for `{module}::{name}` not found");
                            Error::Module
                        })?
                        .memref;
                    linker.define(&store, &module, &name, mem).map_err(|e| {
                        log::trace!("Failed to define `{module}::{name}`: {e}");
                        Error::Module
                    })?;
                }
                ExternType::Func(func_ty) => {
                    let func_ptr = env_def_builder
                        .map
                        .get(&key)
                        .cloned()
                        .and_then(|val| val.host_func())
                        .ok_or_else(|| {
                            log::trace!("Function import for `{module}::{name}` not found");
                            Error::Module
                        })?;

                    let func_ty = func_ty.clone();

                    let func = wasmtime::Func::new(
                        &mut store.inner,
                        func_ty.clone(),
                        move |mut caller, params, results| {
                            let gas = caller.data_mut().gas_global.unwrap_or_else(|| {
                                unreachable!(
                                    "`{GLOBAL_NAME_GAS}` global should be set to `Some(...)`"
                                )
                            });

                            let params: Vec<_> = Some(gas.get(&mut caller))
                                .into_iter()
                                .chain(params.iter().cloned())
                                .map(to_interface)
                                .map(|val| {
                                    val.context("`externref` or `funcref` are not supported")
                                })
                                .collect::<Result<_, _>>()?;

                            let mut caller = Caller(caller);
                            let val = (func_ptr)(&mut caller, &params)
                                .map_err(|HostError| anyhow!("function error"))?;

                            let func_results: Vec<wasmtime::ValType> = func_ty.results().collect();
                            let return_val = match (val.inner, func_results.as_slice()) {
                                (ReturnValue::Unit, []) => None,
                                (ReturnValue::Value(val), [ret]) => {
                                    let val = to_wasmtime(val);
                                    let val_ty = val.ty(&caller).expect("GC is disabled");
                                    anyhow::ensure!(
                                        wasmtime::ValType::eq(&val_ty, ret),
                                        "mismatching return types"
                                    );

                                    Some(val)
                                }
                                results => {
                                    let err_msg = format!(
                                        "Instance::new: embedded executor doesn't support multi-value return. \
                                        Function name - {key:?}, params - {params:?}, results - {results:?}"
                                    );

                                    log::error!("{err_msg}");
                                    unreachable!("{err_msg}")
                                }
                            };

                            gas.set(&mut caller.0, wasmtime::Val::I64(val.gas))
                                .with_context(|| {
                                    format!("failed to set `{GLOBAL_NAME_GAS}` global")
                                })?;

                            if let Some(return_val) = return_val {
                                results[0] = return_val;
                            }

                            Ok(())
                        },
                    );
                    linker.define(&store, &module, &name, func).map_err(|e| {
                        log::trace!("Failed to define `{module}::{name}`: {e}");
                        Error::Module
                    })?;
                }
            }
        }

        let instance = linker.instantiate(&mut *store, &module).map_err(|e| {
            log::trace!(target: TARGET, "Error instantiating module: {e:?}");
            Error::Module
        })?;

        // gas global is optional during some benchmarks
        store.inner.data_mut().gas_global = instance.get_global(&mut *store, GLOBAL_NAME_GAS);

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
        let args = args.iter().cloned().map(to_wasmtime).collect::<Vec<_>>();

        let func = self.instance.get_func(&mut store, name).ok_or_else(|| {
            log::trace!(target: TARGET, "function `{name}` not found");
            Error::Execution
        })?;
        let func_ty = func.ty(&store);
        let mut results = vec![wasmtime::Val::ExternRef(None); func_ty.results().len()];

        func.call(&mut store, &args, &mut results).map_err(|e| {
            log::trace!(target: TARGET, "invocation error: {e}");
            Error::Execution
        })?;

        match results.as_slice() {
            [] => Ok(ReturnValue::Unit),
            [val] => {
                let val = to_interface(*val).ok_or_else(|| {
                    log::trace!(target: TARGET, "error converting return value to interface: {val:?}");
                    Error::Execution
                })?;
                Ok(ReturnValue::Value(val))
            }
            _results => {
                let err_msg = format!(
                    "Instance::invoke: embedded executor doesn't support multi-value return. \
                    Function name - {name:?}, params - {args:?}, results - {_results:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
        }
    }

    fn get_global_val(&self, store: &mut Store<State>, name: &str) -> Option<Value> {
        let global = self.instance.get_global(&mut *store, name)?;
        let global = global.get(store);
        to_interface(global)
    }

    fn set_global_val(
        &self,
        store: &mut Store<State>,
        name: &str,
        value: Value,
    ) -> Result<(), GlobalsSetError> {
        let global = self
            .instance
            .get_global(&mut *store, name)
            .ok_or(GlobalsSetError::NotFound)?;
        global
            .set(store, to_wasmtime(value))
            .map_err(|_| GlobalsSetError::Other)?;
        Ok(())
    }

    fn get_instance_ptr(&self) -> HostPointer {
        let err_msg = "Must not be called for embedded executor";

        log::error!("{err_msg}");
        unreachable!("{err_msg}")
    }
}

/// Convert the substrate value type to the wasmtime value type.
fn to_wasmtime(value: Value) -> wasmtime::Val {
    match value {
        Value::I32(val) => wasmtime::Val::I32(val),
        Value::I64(val) => wasmtime::Val::I64(val),
        Value::F32(val) => wasmtime::Val::F32(val),
        Value::F64(val) => wasmtime::Val::F64(val),
    }
}

/// Convert the wasmtime value type to the substrate value type.
fn to_interface(value: wasmtime::Val) -> Option<Value> {
    match value {
        wasmtime::Val::I32(val) => Some(Value::I32(val)),
        wasmtime::Val::I64(val) => Some(Value::I64(val)),
        wasmtime::Val::F32(val) => Some(Value::F32(val)),
        wasmtime::Val::F64(val) => Some(Value::F64(val)),
        wasmtime::Val::V128(_)
        | wasmtime::Val::FuncRef(_)
        | wasmtime::Val::ExternRef(_)
        | wasmtime::Val::AnyRef(_)
        | wasmtime::Val::ContRef(_)
        | wasmtime::Val::ExnRef(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Caller, EnvironmentDefinitionBuilder, Instance};
    use crate::{
        AsContextExt, Error, HostError, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance,
        SandboxStore, Value, default_executor::Store,
    };
    use assert_matches::assert_matches;
    use gear_sandbox_env::{GLOBAL_NAME_GAS, WasmReturnValue};

    fn execute_sandboxed(code: &[u8], args: &[Value]) -> Result<ReturnValue, Error> {
        struct State {
            counter: u32,
        }

        fn env_assert(
            _c: &mut Caller<'_, State>,
            args: &[Value],
        ) -> Result<WasmReturnValue, HostError> {
            if args.len() != 2 {
                return Err(HostError);
            }
            let condition = args[1].as_i32().ok_or(HostError)?;
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
            e: &mut Caller<'_, State>,
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
            _c: &mut Caller<'_, State>,
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
        let code = wat::parse_str(format!(
            r#"
		(module
			(import "env" "assert" (func $assert (param i32)))
			(global (;0;) (mut i64) (i64.const 0x20000))
			(export "{GLOBAL_NAME_GAS}" (global 0))

			(func (export "call") (param $x i32) (param $y i64)
				;; assert that $x = 0x12345678
				(call $assert
					(i32.eq
						(local.get $x)
						(i32.const 0x12345678)
					)
				)

				(call $assert
					(i64.eq
						(local.get $y)
						(i64.const 0x1234567887654321)
					)
				)
			)
		)
		"#
        ))
        .unwrap();

        execute_sandboxed(
            &code,
            &[Value::I32(0x12345678), Value::I64(0x1234567887654321)],
        )
        .unwrap();
    }

    #[test]
    fn return_value() {
        let code = wat::parse_str(format!(
            r#"
		(module
		    (global (;0;) (mut i64) (i64.const 0x20000))
			(export "{GLOBAL_NAME_GAS}" (global 0))

			(func (export "call") (param $x i32) (result i32)
				(i32.add
					(local.get $x)
					(i32.const 1)
				)
			)
		)
		"#
        ))
        .unwrap();

        let return_val = execute_sandboxed(&code, &[Value::I32(0x1336)]).unwrap();
        assert_eq!(return_val, ReturnValue::Value(Value::I32(0x1337)));
    }

    #[test]
    fn cant_return_unmatching_type() {
        fn env_returns_i32(
            _e: &mut Caller<'_, ()>,
            _args: &[Value],
        ) -> Result<WasmReturnValue, HostError> {
            Ok(WasmReturnValue {
                gas: 0,
                inner: ReturnValue::Value(Value::I32(42)),
            })
        }

        let mut env_builder = EnvironmentDefinitionBuilder::new();
        env_builder.add_host_func("env", "returns_i32", env_returns_i32);

        let code = wat::parse_str(format!(
            r#"
		(module
			;; It's actually returns i32, but imported as if it returned i64
			(import "env" "returns_i32" (func $returns_i32 (result i64)))
			(global (;0;) (mut i64) (i64.const 0x20000))
			(export "{GLOBAL_NAME_GAS}" (global 0))

			(func (export "call")
				(drop
					(call $returns_i32)
				)
			)
		)
		"#
        ))
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
