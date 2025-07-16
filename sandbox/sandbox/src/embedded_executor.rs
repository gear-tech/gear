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

//! An embedded WASM executor utilizing `wasmer`.

use crate::{
    AsContextExt, Error, GlobalsSetError, HostError, HostFuncType, ReturnValue, SandboxStore, Value,
};
use alloc::string::String;
use gear_sandbox_env::GLOBAL_NAME_GAS;
use sp_wasm_interface_common::HostPointer;
use std::{
    collections::btree_map::BTreeMap, env, fs, marker::PhantomData, path::PathBuf, ptr::NonNull,
    sync::OnceLock,
};
use wasmer::{
    sys::{BaseTunables, VMConfig},
    vm::{
        LinearMemory, MemoryStyle, TableStyle, VMGlobal, VMMemory, VMMemoryDefinition, VMTable,
        VMTableDefinition,
    },
    Engine, FunctionEnv, Global, GlobalType, Imports, MemoryError, MemoryType, NativeEngineExt,
    RuntimeError, StoreMut, StoreObjects, StoreRef, TableType, Target, Tunables,
    Value as RuntimeValue,
};
use wasmer_types::ExternType;

/// The target used for logging.
const TARGET: &str = "runtime::sandbox";

fn cache_base_path() -> PathBuf {
    static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
    CACHE_DIR
        .get_or_init(|| {
            // We acquire workspace root dir during runtime and compile-time.
            //
            // During development, runtime workspace dir equals to compile-time one,
            // so all compiled WASMs are cached in usual `OUT_DIR`
            // like we don't rewrite it.
            //
            // During cross-compile, runtime workspace dir differs from compile-time one and
            // accordingly `OUT_DIR` beginning differs too,
            // so we change its beginning to successfully run tests.
            //
            // `OUT_DIR` is used for caching instead of some platform-specific project folder to
            // not maintain ever-growing number of cached WASMs

            let runtime_workspace_dir = PathBuf::from(env::var("GEAR_WORKSPACE_DIR").unwrap());
            let compiled_workspace_dir = PathBuf::from(env!("GEAR_WORKSPACE_DIR"));

            let out_dir = PathBuf::from(env!("OUT_DIR"));
            let out_dir = pathdiff::diff_paths(out_dir, compiled_workspace_dir).unwrap();
            let out_dir = runtime_workspace_dir.join(out_dir);

            let cache = out_dir.join("wasmer-cache");
            fs::create_dir_all(&cache).unwrap();
            cache
        })
        .into()
}

struct CustomTunables {
    inner: BaseTunables,
    vmconfig: VMConfig,
}

impl CustomTunables {
    fn for_target(target: &Target) -> Self {
        Self {
            inner: BaseTunables::for_target(target),
            vmconfig: VMConfig {
                wasm_stack_size: None,
            },
        }
    }

    fn with_wasm_stack_size(mut self, wasm_stack_size: impl Into<Option<usize>>) -> Self {
        self.vmconfig.wasm_stack_size = wasm_stack_size.into();
        self
    }
}

impl Tunables for CustomTunables {
    fn memory_style(&self, memory: &MemoryType) -> MemoryStyle {
        self.inner.memory_style(memory)
    }

    fn table_style(&self, table: &TableType) -> TableStyle {
        self.inner.table_style(table)
    }

    fn create_host_memory(
        &self,
        ty: &MemoryType,
        style: &MemoryStyle,
    ) -> Result<VMMemory, MemoryError> {
        self.inner.create_host_memory(ty, style)
    }

    unsafe fn create_vm_memory(
        &self,
        ty: &MemoryType,
        style: &MemoryStyle,
        vm_definition_location: NonNull<VMMemoryDefinition>,
    ) -> Result<VMMemory, MemoryError> {
        unsafe {
            self.inner
                .create_vm_memory(ty, style, vm_definition_location)
        }
    }

    fn create_host_table(&self, ty: &TableType, style: &TableStyle) -> Result<VMTable, String> {
        self.inner.create_host_table(ty, style)
    }

    unsafe fn create_vm_table(
        &self,
        ty: &TableType,
        style: &TableStyle,
        vm_definition_location: NonNull<VMTableDefinition>,
    ) -> Result<VMTable, String> {
        unsafe {
            self.inner
                .create_vm_table(ty, style, vm_definition_location)
        }
    }

    fn create_global(&self, ty: GlobalType) -> Result<VMGlobal, String> {
        self.inner.create_global(ty)
    }

    unsafe fn create_memories(
        &self,
        context: &mut StoreObjects,
        module: &wasmer_types::ModuleInfo,
        memory_styles: &wasmer_types::entity::PrimaryMap<wasmer_types::MemoryIndex, MemoryStyle>,
        memory_definition_locations: &[NonNull<VMMemoryDefinition>],
    ) -> Result<
        wasmer_types::entity::PrimaryMap<
            wasmer_types::LocalMemoryIndex,
            wasmer_vm::InternalStoreHandle<VMMemory>,
        >,
        wasmer_compiler::LinkError,
    > {
        unsafe {
            self.inner
                .create_memories(context, module, memory_styles, memory_definition_locations)
        }
    }

    unsafe fn create_tables(
        &self,
        context: &mut StoreObjects,
        module: &wasmer_types::ModuleInfo,
        table_styles: &wasmer_types::entity::PrimaryMap<wasmer_types::TableIndex, TableStyle>,
        table_definition_locations: &[NonNull<VMTableDefinition>],
    ) -> Result<
        wasmer_types::entity::PrimaryMap<
            wasmer_types::LocalTableIndex,
            wasmer_vm::InternalStoreHandle<VMTable>,
        >,
        wasmer_compiler::LinkError,
    > {
        unsafe {
            self.inner
                .create_tables(context, module, table_styles, table_definition_locations)
        }
    }

    fn create_globals(
        &self,
        context: &mut StoreObjects,
        module: &wasmer_types::ModuleInfo,
    ) -> Result<
        wasmer_types::entity::PrimaryMap<
            wasmer_types::LocalGlobalIndex,
            wasmer_vm::InternalStoreHandle<VMGlobal>,
        >,
        wasmer_compiler::LinkError,
    > {
        self.inner.create_globals(context, module)
    }

    fn vmconfig(&self) -> &VMConfig {
        &self.vmconfig
    }
}

/// [`AsContextExt`] extension.
pub trait AsContext: wasmer::AsStoreRef + wasmer::AsStoreMut {}

#[derive(Debug)]
struct InnerState<T> {
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

/// wasmer store wrapper.
#[derive(Debug)]
pub struct Store<T> {
    inner: wasmer::Store,
    state: FunctionEnv<InnerState<T>>,
}

impl<T> Store<T> {
    fn engine(&self) -> &Engine {
        self.inner.engine()
    }
}

impl<T: Send + 'static> SandboxStore for Store<T> {
    fn new(state: T) -> Self {
        let mut engine = Engine::from(wasmer::Singlepass::new());
        let tunables = CustomTunables::for_target(engine.target())
            // make stack size bigger for fuzzer
            .with_wasm_stack_size(16 * 1024 * 1024);
        engine.set_tunables(tunables);
        let mut store = wasmer::Store::new(engine);

        let state = FunctionEnv::new(&mut store, InnerState::new(state));

        Self {
            inner: store,
            state,
        }
    }
}

impl<T> wasmer::AsStoreRef for Store<T> {
    fn as_store_ref(&self) -> StoreRef<'_> {
        self.inner.as_store_ref()
    }
}

impl<T> wasmer::AsStoreMut for Store<T> {
    fn as_store_mut(&mut self) -> StoreMut<'_> {
        self.inner.as_store_mut()
    }

    fn objects_mut(&mut self) -> &mut StoreObjects {
        self.inner.objects_mut()
    }
}

impl<T: Send + 'static> AsContextExt for Store<T> {
    type State = T;

    fn data_mut(&mut self) -> &mut Self::State {
        &mut self.state.as_mut(&mut self.inner).inner
    }
}

impl<T> AsContext for Store<T> {}

/// wasmer function env wrapper.
pub struct Caller<'a, T>(wasmer::FunctionEnvMut<'a, InnerState<T>>);

impl<T> wasmer::AsStoreRef for Caller<'_, T> {
    fn as_store_ref(&self) -> StoreRef<'_> {
        self.0.as_store_ref()
    }
}

impl<T> wasmer::AsStoreMut for Caller<'_, T> {
    fn as_store_mut(&mut self) -> StoreMut<'_> {
        self.0.as_store_mut()
    }

    fn objects_mut(&mut self) -> &mut StoreObjects {
        self.0.objects_mut()
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
    memref: wasmer::Memory,
    base: usize,
}

impl<T> super::SandboxMemory<T> for Memory {
    fn new(store: &mut Store<T>, initial: u32, maximum: Option<u32>) -> Result<Memory, Error> {
        let ty = MemoryType::new(initial, maximum, false);
        let memory_style = store.engine().tunables().memory_style(&ty);
        let memref = VMMemory::new(&ty, &memory_style).map_err(|e| {
            log::trace!("Failed to create memory: {e}");
            Error::Module
        })?;
        // SAFETY: `vmmemory()` returns `NonNull` so pointer is valid
        let memory_definition = unsafe { memref.vmmemory().as_ref() };
        let base = memory_definition.base as usize;
        let memref = wasmer::Memory::new_from_existing(store, memref);
        Ok(Memory { memref, base })
    }

    fn read<Context>(&self, ctx: &Context, ptr: u32, buf: &mut [u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .view(ctx)
            .read(ptr as u64, buf)
            .map_err(|_| Error::OutOfBounds)?;
        Ok(())
    }

    fn write<Context>(&self, ctx: &mut Context, ptr: u32, value: &[u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .view(ctx)
            .write(ptr as u64, value)
            .map_err(|_| Error::OutOfBounds)?;
        Ok(())
    }

    fn grow<Context>(&self, ctx: &mut Context, pages: u32) -> Result<u32, Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.memref
            .grow(ctx, pages)
            .map(|pages| pages.0)
            .map_err(|_| Error::MemoryGrow)
    }

    fn size<Context>(&self, ctx: &Context) -> u32
    where
        Context: AsContextExt<State = T>,
    {
        self.memref.view(ctx).size().0
    }

    unsafe fn get_buff<Context>(&self, _ctx: &Context) -> u64
    where
        Context: AsContextExt<State = T>,
    {
        self.base as u64
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
    instance: wasmer::Instance,
    _marker: PhantomData<State>,
}

impl<State> Clone for Instance<State> {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance.clone(),
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
        let module = gear_wasmer_cache::get(store.engine(), code, cache_base_path())
            .inspect_err(|e| log::trace!(target: TARGET, "Failed to create module: {e}"))
            .map_err(|_e| Error::Module)?;
        let mut imports = Imports::new();

        for import in module.imports() {
            let module = import.module().to_string();
            let name = import.name().to_string();
            let key = (module.clone(), name.clone());

            match import.ty() {
                ExternType::Global(_) | ExternType::Table(_) => {}
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
                    imports.define(&module, &name, mem);
                }
                ExternType::Function(func_ty) => {
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

                    let func = wasmer::Function::new_with_env(
                        &mut store.inner,
                        &store.state,
                        func_ty.clone(),
                        move |mut env, params| {
                            let (inner_state, mut store) = env.data_and_store_mut();
                            let gas = inner_state
                                .gas_global
                                .as_ref()
                                .unwrap_or_else(|| {
                                    unreachable!(
                                        "`{GLOBAL_NAME_GAS}` global should be set to `Some(...)`"
                                    )
                                })
                                .clone();

                            let params: Vec<_> = Some(gas.get(&mut store))
                                .into_iter()
                                .chain(params.iter().cloned())
                                .map(to_interface)
                                .map(|val| {
                                    val.ok_or_else(|| {
                                        RuntimeError::new(
                                            "`externref` or `funcref` are not supported",
                                        )
                                    })
                                })
                                .collect::<Result<_, _>>()?;

                            let mut caller = Caller(env);
                            let val = (func_ptr)(&mut caller, &params)
                                .map_err(|HostError| RuntimeError::new("function error"))?;

                            let return_val = match (val.inner, func_ty.results()) {
                                (ReturnValue::Unit, []) => None,
                                (ReturnValue::Value(val), [ret]) => {
                                    let val = to_wasmer(val);

                                    if val.ty() != *ret {
                                        return Err(RuntimeError::new("mismatching return types"));
                                    }

                                    Some(val)
                                }
                                _results => {
                                    let err_msg = format!(
                                        "Instance::new: embedded executor doesn't support multi-value return. \
                                        Function name - {key:?}, params - {params:?}, results - {_results:?}"
                                    );

                                    log::error!("{err_msg}");
                                    unreachable!("{err_msg}")
                                }
                            };

                            gas.set(&mut caller.0, RuntimeValue::I64(val.gas))
                                .map_err(|e| {
                                    RuntimeError::new(format!(
                                        "failed to set `{GLOBAL_NAME_GAS}` global: {e}"
                                    ))
                                })?;

                            Ok(Vec::from_iter(return_val))
                        },
                    );
                    imports.define(&module, &name, func);
                }
            }
        }

        let instance = wasmer::Instance::new(store, &module, &imports).map_err(|e| {
            log::trace!(target: TARGET, "Error instantiating module: {e:?}");
            Error::Module
        })?;

        store.state.as_mut(&mut store.inner).gas_global = instance
            .exports
            .get_global(GLOBAL_NAME_GAS)
            // gas global is optional during some benchmarks
            .ok()
            .cloned();

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
        let args = args.iter().cloned().map(to_wasmer).collect::<Vec<_>>();

        let func = self.instance.exports.get_function(name).map_err(|e| {
            log::trace!(target: TARGET, "function `{name}` not found: {e}");
            Error::Execution
        })?;

        let results = func.call(&mut store, &args).map_err(|e| {
            log::trace!(target: TARGET, "invocation error: {e}");
            Error::Execution
        })?;

        match results.as_ref() {
            [] => Ok(ReturnValue::Unit),
            [val] => {
                let val = to_interface(val.clone()).ok_or_else(|| {
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
        let global = self.instance.exports.get_global(name).ok()?;
        let global = global.get(store);
        to_interface(global)
    }

    fn set_global_val(
        &self,
        mut store: &mut Store<State>,
        name: &str,
        value: Value,
    ) -> Result<(), GlobalsSetError> {
        let global = self
            .instance
            .exports
            .get_global(name)
            .map_err(|_| GlobalsSetError::NotFound)?;
        global
            .set(&mut store, to_wasmer(value))
            .map_err(|_| GlobalsSetError::Other)?;
        Ok(())
    }

    fn get_instance_ptr(&self) -> HostPointer {
        let err_msg = "Must not be called for embedded executor";

        log::error!("{err_msg}");
        unreachable!("{err_msg}")
    }
}

/// Convert the substrate value type to the wasmer value type.
fn to_wasmer(value: Value) -> RuntimeValue {
    match value {
        Value::I32(val) => RuntimeValue::I32(val),
        Value::I64(val) => RuntimeValue::I64(val),
        Value::F32(val) => RuntimeValue::F32(f32::from_bits(val)),
        Value::F64(val) => RuntimeValue::F64(f64::from_bits(val)),
    }
}

/// Convert the wasmer value type to the substrate value type.
fn to_interface(value: RuntimeValue) -> Option<Value> {
    match value {
        RuntimeValue::I32(val) => Some(Value::I32(val)),
        RuntimeValue::I64(val) => Some(Value::I64(val)),
        RuntimeValue::F32(val) => Some(Value::F32(val.to_bits())),
        RuntimeValue::F64(val) => Some(Value::F64(val.to_bits())),
        RuntimeValue::V128(_) | RuntimeValue::FuncRef(_) | RuntimeValue::ExternRef(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Caller, EnvironmentDefinitionBuilder, Instance};
    use crate::{
        default_executor::Store, AsContextExt, Error, HostError, ReturnValue,
        SandboxEnvironmentBuilder, SandboxInstance, SandboxStore, Value,
    };
    use assert_matches::assert_matches;
    use gear_sandbox_env::{WasmReturnValue, GLOBAL_NAME_GAS};

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
