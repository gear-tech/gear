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

//! This module implements sandboxing support in the runtime.
//!
//! Sandboxing is backed by wasmi and wasmer, depending on the configuration.

mod wasmer_backend;
mod wasmi_backend;

use std::{cell::RefCell, collections::HashMap, pin::Pin, rc::Rc};

use codec::Decode;
use env::Instantiate;
use gear_sandbox_env as sandbox_env;
use sp_wasm_interface_common::{Pointer, Value, WordSize};

use crate::{
    error::{self, Result},
    util,
};

use self::{
    wasmer_backend::{
        get_global as wasmer_get_global, instantiate as wasmer_instantiate,
        invoke as wasmer_invoke, new_memory as wasmer_new_memory, set_global as wasmer_set_global,
        Backend as WasmerBackend, MemoryWrapper as WasmerMemoryWrapper,
    },
    wasmi_backend::{
        get_global as wasmi_get_global, instantiate as wasmi_instantiate, invoke as wasmi_invoke,
        new_memory as wasmi_new_memory, set_global as wasmi_set_global,
        MemoryWrapper as WasmiMemoryWrapper,
    },
};

pub use gear_sandbox_env as env;

type SandboxResult<T> = core::result::Result<T, String>;

/// Index of a function inside the supervisor.
///
/// This is a typically an index in the default table of the supervisor, however
/// the exact meaning of this index is depends on the implementation of dispatch function.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SupervisorFuncIndex(usize);

impl From<SupervisorFuncIndex> for usize {
    fn from(index: SupervisorFuncIndex) -> Self {
        index.0
    }
}

/// Index of a function within guest index space.
///
/// This index is supposed to be used as index for `Externals`.
#[derive(Copy, Clone, Debug, PartialEq)]
struct GuestFuncIndex(usize);

/// This struct holds a mapping from guest index space to supervisor.
struct GuestToSupervisorFunctionMapping {
    /// Position of elements in this vector are interpreted
    /// as indices of guest functions and are mapped to
    /// corresponding supervisor function indices.
    funcs: Vec<SupervisorFuncIndex>,
}

impl GuestToSupervisorFunctionMapping {
    /// Create an empty function mapping
    fn new() -> GuestToSupervisorFunctionMapping {
        GuestToSupervisorFunctionMapping { funcs: Vec::new() }
    }

    /// Add a new supervisor function to the mapping.
    /// Returns a newly assigned guest function index.
    fn define(&mut self, supervisor_func: SupervisorFuncIndex) -> GuestFuncIndex {
        let idx = self.funcs.len();
        self.funcs.push(supervisor_func);
        GuestFuncIndex(idx)
    }

    /// Find supervisor function index by its corresponding guest function index
    fn func_by_guest_index(&self, guest_func_idx: GuestFuncIndex) -> Option<SupervisorFuncIndex> {
        self.funcs.get(guest_func_idx.0).cloned()
    }
}

/// Holds sandbox function and memory imports and performs name resolution
struct Imports {
    /// Maps qualified function name to its guest function index
    func_map: HashMap<(String, String), GuestFuncIndex>,

    /// Maps qualified field name to its memory reference
    memories_map: HashMap<(String, String), Memory>,
}

impl Imports {
    fn func_by_name(&self, module_name: &str, func_name: &str) -> Option<GuestFuncIndex> {
        self.func_map
            .get(&(module_name.to_owned(), func_name.to_string()))
            .cloned()
    }

    fn memory_by_name(&self, module_name: &str, memory_name: &str) -> Option<Memory> {
        self.memories_map
            .get(&(module_name.to_string(), memory_name.to_string()))
            .cloned()
    }
}

/// The sandbox context used to execute sandboxed functions.
pub trait SandboxContext {
    /// Invoke a function in the supervisor environment.
    ///
    /// This first invokes the dispatch thunk function, passing in the function index of the
    /// desired function to call and serialized arguments. The thunk calls the desired function
    /// with the deserialized arguments, then serializes the result into memory and returns
    /// reference. The pointer to and length of the result in linear memory is encoded into an
    /// `i64`, with the upper 32 bits representing the pointer and the lower 32 bits representing
    /// the length.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the dispatch_thunk function has an incorrect signature or traps during
    /// execution.
    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: SupervisorFuncIndex,
    ) -> Result<i64>;

    /// Read memory from `address` into a vector.
    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> SandboxResult<()>;

    /// Read memory into the given `dest` buffer from `address`.
    fn read_memory(&self, address: Pointer<u8>, size: WordSize) -> Result<Vec<u8>> {
        let mut vec = vec![0; size as usize];
        self.read_memory_into(address, &mut vec)?;
        Ok(vec)
    }

    /// Write the given data at `address` into the memory.
    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> SandboxResult<()>;

    /// Allocate a memory instance of `size` bytes.
    fn allocate_memory(&mut self, size: WordSize) -> SandboxResult<Pointer<u8>>;

    /// Deallocate a given memory instance.
    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> SandboxResult<()>;
}

/// Implementation of [`Externals`] that allows execution of guest module with
/// [externals][`Externals`] that might refer functions defined by supervisor.
///
/// [`Externals`]: ../wasmi/trait.Externals.html
pub struct GuestExternals<'a> {
    /// Instance of sandboxed module to be dispatched
    sandbox_instance: &'a SandboxInstance,
}

/// Module instance in terms of selected backend
enum BackendInstance {
    /// Wasmi module instance
    Wasmi(wasmi::ModuleRef),

    /// Wasmer module instance
    Wasmer(sandbox_wasmer::Instance),
}

/// Sandboxed instance of a wasm module.
///
/// It's primary purpose is to [`invoke`] exported functions on it.
///
/// All imports of this instance are specified at the creation time and
/// imports are implemented by the supervisor.
///
/// Hence, in order to invoke an exported function on a sandboxed module instance,
/// it's required to provide supervisor externals: it will be used to execute
/// code in the supervisor context.
///
/// This is generic over a supervisor function reference type.
///
/// [`invoke`]: #method.invoke
pub struct SandboxInstance {
    backend_instance: BackendInstance,
    guest_to_supervisor_mapping: GuestToSupervisorFunctionMapping,
}

impl SandboxInstance {
    /// Invoke an exported function by a name.
    ///
    /// `supervisor_externals` is required to execute the implementations
    /// of the syscalls that published to a sandboxed module instance.
    pub fn invoke(
        &self,
        backned_context: &mut BackendContext,
        export_name: &str,
        args: &[Value],
        sandbox_context: &mut dyn SandboxContext,
    ) -> std::result::Result<Option<Value>, error::Error> {
        match (&self.backend_instance, backned_context) {
            (BackendInstance::Wasmi(wasmi_instance), BackendContext::Wasmi) => {
                wasmi_invoke(self, wasmi_instance, export_name, args, sandbox_context)
            }

            (BackendInstance::Wasmer(wasmer_instance), BackendContext::Wasmer(wasmer_backend)) => {
                wasmer_invoke(
                    wasmer_instance,
                    wasmer_backend.store_mut(),
                    export_name,
                    args,
                    sandbox_context,
                )
            }
            _ => unimplemented!("Mismatch between backend instance and context"),
        }
    }

    /// Get the value from a global with the given `name`.
    ///
    /// Returns `Some(_)` if the global could be found.
    pub fn get_global_val(
        &self,
        backned_context: &mut BackendContext,
        name: &str,
    ) -> Option<Value> {
        match (&self.backend_instance, backned_context) {
            (BackendInstance::Wasmi(wasmi_instance), BackendContext::Wasmi) => {
                wasmi_get_global(wasmi_instance, name)
            }

            (BackendInstance::Wasmer(wasmer_instance), BackendContext::Wasmer(wasmer_backend)) => {
                wasmer_get_global(wasmer_instance, wasmer_backend.store_mut(), name)
            }

            _ => unimplemented!("Mismatch between backend instance and context"),
        }
    }

    /// Set the value of a global with the given `name`.
    ///
    /// Returns `Ok(Some(()))` if the global could be modified.
    pub fn set_global_val(
        &self,
        backned_context: &mut BackendContext,
        name: &str,
        value: Value,
    ) -> std::result::Result<Option<()>, error::Error> {
        match (&self.backend_instance, backned_context) {
            (BackendInstance::Wasmi(wasmi_instance), BackendContext::Wasmi) => {
                wasmi_set_global(wasmi_instance, name, value)
            }

            (BackendInstance::Wasmer(wasmer_instance), BackendContext::Wasmer(wasmer_backend)) => {
                wasmer_set_global(wasmer_instance, wasmer_backend.store_mut(), name, value)
            }

            _ => unimplemented!("Mismatch between backend instance and context"),
        }
    }
}

/// Error occurred during instantiation of a sandboxed module.
pub enum InstantiationError {
    /// Something wrong with the environment definition. It either can't
    /// be decoded, have a reference to a non-existent or torn down memory instance.
    EnvironmentDefinitionCorrupted,
    /// Provided module isn't recognized as a valid webassembly binary.
    ModuleDecoding,
    /// Module is a well-formed webassembly binary but could not be instantiated. This could
    /// happen because, e.g. the module imports entries not provided by the environment.
    Instantiation,
    /// Module is well-formed, instantiated and linked, but while executing the start function
    /// a trap was generated.
    StartTrapped,
    /// The code was compiled with a CPU feature not available on the host.
    CpuFeature,
}

fn decode_environment_definition(
    mut raw_env_def: &[u8],
    memories: &[Option<Memory>],
) -> std::result::Result<(Imports, GuestToSupervisorFunctionMapping), InstantiationError> {
    let env_def = sandbox_env::EnvironmentDefinition::decode(&mut raw_env_def)
        .map_err(|_| InstantiationError::EnvironmentDefinitionCorrupted)?;

    let mut func_map = HashMap::new();
    let mut memories_map = HashMap::new();
    let mut guest_to_supervisor_mapping = GuestToSupervisorFunctionMapping::new();

    for entry in &env_def.entries {
        let module = entry.module_name.clone();
        let field = entry.field_name.clone();

        match entry.entity {
            sandbox_env::ExternEntity::Function(func_idx) => {
                let externals_idx =
                    guest_to_supervisor_mapping.define(SupervisorFuncIndex(func_idx as usize));
                func_map.insert((module, field), externals_idx);
            }
            sandbox_env::ExternEntity::Memory(memory_idx) => {
                let memory_ref = memories
                    .get(memory_idx as usize)
                    .cloned()
                    .ok_or(InstantiationError::EnvironmentDefinitionCorrupted)?
                    .ok_or(InstantiationError::EnvironmentDefinitionCorrupted)?;
                memories_map.insert((module, field), memory_ref);
            }
        }
    }

    Ok((
        Imports {
            func_map,
            memories_map,
        },
        guest_to_supervisor_mapping,
    ))
}

/// An environment in which the guest module is instantiated.
pub struct GuestEnvironment {
    /// Function and memory imports of the guest module
    imports: Imports,

    /// Supervisor functinons mapped to guest index space
    guest_to_supervisor_mapping: GuestToSupervisorFunctionMapping,
}

impl GuestEnvironment {
    /// Decodes an environment definition from the given raw bytes.
    ///
    /// Returns `Err` if the definition cannot be decoded.
    pub fn decode<DT>(
        store: &Store<DT>,
        raw_env_def: &[u8],
    ) -> std::result::Result<Self, InstantiationError> {
        let (imports, guest_to_supervisor_mapping) =
            decode_environment_definition(raw_env_def, &store.memories)?;
        Ok(Self {
            imports,
            guest_to_supervisor_mapping,
        })
    }
}

/// An unregistered sandboxed instance.
///
/// To finish off the instantiation the user must call `register`.
#[must_use]
pub struct UnregisteredInstance {
    sandbox_instance: SandboxInstance,
}

impl UnregisteredInstance {
    /// Finalizes instantiation of this module.
    pub fn register<DT>(self, store: &mut Store<DT>, dispatch_thunk: DT) -> u32 {
        // At last, register the instance.
        store.register_sandbox_instance(self.sandbox_instance, dispatch_thunk)
    }
}

/// Sandbox backend to use
pub enum SandboxBackend {
    /// Wasm interpreter
    Wasmi,

    /// Wasmer environment
    Wasmer,
}

/// Memory reference in terms of a selected backend
#[derive(Clone, Debug)]
pub enum Memory {
    /// Wasmi memory reference
    Wasmi(WasmiMemoryWrapper),

    /// Wasmer memory reference
    Wasmer(WasmerMemoryWrapper),
}

impl Memory {
    /// View as wasmi memory
    pub fn as_wasmi(&self) -> Option<WasmiMemoryWrapper> {
        match self {
            Memory::Wasmi(memory) => Some(memory.clone()),

            Memory::Wasmer(_) => None,
        }
    }

    /// View as wasmer memory
    pub fn as_wasmer(&self) -> Option<WasmerMemoryWrapper> {
        match self {
            Memory::Wasmer(memory) => Some(memory.clone()),
            Memory::Wasmi(_) => None,
        }
    }
}

impl util::MemoryTransfer for Memory {
    fn read(
        &self,
        backend_context: &BackendContext,
        source_addr: Pointer<u8>,
        size: usize,
    ) -> Result<Vec<u8>> {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => {
                sandboxed_memory.read(source_addr, size)
            }

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.read(wasmer_backend.store(), source_addr, size)
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }

    fn read_into(
        &self,
        backend_context: &BackendContext,
        source_addr: Pointer<u8>,
        destination: &mut [u8],
    ) -> Result<()> {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => {
                sandboxed_memory.read_into(source_addr, destination)
            }

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.read_into(wasmer_backend.store(), source_addr, destination)
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }

    fn write_from(
        &self,
        backend_context: &BackendContext,
        dest_addr: Pointer<u8>,
        source: &[u8],
    ) -> Result<()> {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => {
                sandboxed_memory.write_from(dest_addr, source)
            }

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.write_from(wasmer_backend.store(), dest_addr, source)
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }

    fn memory_grow(&mut self, backend_context: &mut BackendContext, pages: u32) -> Result<u32> {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => {
                sandboxed_memory.memory_grow(pages)
            }

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.memory_grow(wasmer_backend.store_mut(), pages)
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }

    fn memory_size(&self, backend_context: &BackendContext) -> u32 {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => {
                sandboxed_memory.memory_size()
            }

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.memory_size(wasmer_backend.store())
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }

    fn get_buff(&self, backend_context: &BackendContext) -> *mut u8 {
        match (self, backend_context) {
            (Memory::Wasmi(sandboxed_memory), BackendContext::Wasmi) => sandboxed_memory.get_buff(),

            (Memory::Wasmer(sandboxed_memory), BackendContext::Wasmer(wasmer_backend)) => {
                sandboxed_memory.get_buff(wasmer_backend.store())
            }

            _ => unimplemented!("Mismatch between memory instance and backend context"),
        }
    }
}

/// Information specific to a particular execution backend
pub enum BackendContext {
    /// Wasmi specific context
    Wasmi,

    /// Wasmer specific context
    Wasmer(WasmerBackend),
}

impl BackendContext {
    /// Create a new backend context
    pub fn new(backend: SandboxBackend) -> BackendContext {
        match backend {
            SandboxBackend::Wasmi => BackendContext::Wasmi,

            SandboxBackend::Wasmer => BackendContext::Wasmer(WasmerBackend::new()),
        }
    }
}

/// This struct keeps track of all sandboxed components.
///
/// This is generic over a supervisor function reference type.
pub struct Store<DT> {
    /// Stores the instance and the dispatch thunk associated to per instance.
    ///
    /// Instances are `Some` until torn down.
    instances: Vec<Option<(Pin<Rc<SandboxInstance>>, DT)>>,
    /// Memories are `Some` until torn down.
    memories: Vec<Option<Memory>>,
    backend_context: Rc<RefCell<BackendContext>>,
}

impl<DT: Clone> Store<DT> {
    /// Create a new empty sandbox store.
    pub fn new(backend: SandboxBackend) -> Self {
        Store {
            instances: Vec::new(),
            memories: Vec::new(),
            backend_context: Rc::new(RefCell::new(BackendContext::new(backend))),
        }
    }

    /// Clear instance list and memory list.
    pub fn clear(&mut self) {
        log::trace!(
            "clear; instances = {}",
            self.instances.iter().any(|i| i.is_some())
        );
        self.instances.clear();
        log::trace!(
            "clear; memories = {}",
            self.memories.iter().any(|m| m.is_some())
        );
        self.memories.clear();

        match &mut *self.backend_context.borrow_mut() {
            BackendContext::Wasmi => (),
            BackendContext::Wasmer(wasmer_backend) => {
                *wasmer_backend = WasmerBackend::new();
            }
        }
    }

    /// Create a new memory instance and return it's index.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the memory couldn't be created.
    /// Typically happens if `initial` is more than `maximum`.
    pub fn new_memory(&mut self, initial: u32, maximum: u32) -> Result<u32> {
        let memories = &mut self.memories;
        let backend_context = &mut *self.backend_context.borrow_mut();

        let maximum = match maximum {
            sandbox_env::MEM_UNLIMITED => None,
            specified_limit => Some(specified_limit),
        };

        let memory = match backend_context {
            BackendContext::Wasmi => wasmi_new_memory(initial, maximum)?,

            BackendContext::Wasmer(ref mut context) => {
                wasmer_new_memory(context, initial, maximum)?
            }
        };

        let mem_idx = memories.len();
        memories.push(Some(memory));

        Ok(mem_idx as u32)
    }

    /// Returns `SandboxInstance` by `instance_idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err` If `instance_idx` isn't a valid index of an instance or
    /// instance is already torndown.
    #[allow(clippy::useless_asref)]
    pub fn instance(&self, instance_idx: u32) -> Result<Pin<Rc<SandboxInstance>>> {
        self.instances
            .get(instance_idx as usize)
            .ok_or("Trying to access a non-existent instance")?
            .as_ref()
            .map(|v| v.0.clone())
            .ok_or_else(|| "Trying to access a torndown instance".into())
    }

    /// Returns dispatch thunk by `instance_idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err` If `instance_idx` isn't a valid index of an instance or
    /// instance is already torndown.
    #[allow(clippy::useless_asref)]
    pub fn dispatch_thunk(&self, instance_idx: u32) -> Result<DT> {
        self.instances
            .get(instance_idx as usize)
            .as_ref()
            .ok_or("Trying to access a non-existent instance")?
            .as_ref()
            .map(|v| v.1.clone())
            .ok_or_else(|| "Trying to access a torndown instance".into())
    }

    /// Returns reference to a memory instance by `memory_idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err` If `memory_idx` isn't a valid index of an memory or
    /// if memory has been torn down.
    pub fn memory(&self, memory_idx: u32) -> Result<Memory> {
        self.memories
            .get(memory_idx as usize)
            .cloned()
            .ok_or("Trying to access a non-existent sandboxed memory")?
            .ok_or_else(|| "Trying to access a torndown sandboxed memory".into())
    }

    /// Tear down the memory at the specified index.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `memory_idx` isn't a valid index of an memory or
    /// if it has been torn down.
    pub fn memory_teardown(&mut self, memory_idx: u32) -> Result<()> {
        match self.memories.get_mut(memory_idx as usize) {
            None => Err("Trying to teardown a non-existent sandboxed memory".into()),
            Some(None) => Err("Double teardown of a sandboxed memory".into()),
            Some(memory) => {
                *memory = None;
                Ok(())
            }
        }
    }

    /// Tear down the instance at the specified index.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `instance_idx` isn't a valid index of an instance or
    /// if it has been torn down.
    pub fn instance_teardown(&mut self, instance_idx: u32) -> Result<()> {
        match self.instances.get_mut(instance_idx as usize) {
            None => Err("Trying to teardown a non-existent instance".into()),
            Some(None) => Err("Double teardown of an instance".into()),
            Some(instance) => {
                *instance = None;
                Ok(())
            }
        }
    }

    /// Instantiate a guest module and return it's index in the store.
    ///
    /// The guest module's code is specified in `wasm`. Environment that will be available to
    /// guest module is specified in `guest_env`. A dispatch thunk is used as function that
    /// handle calls from guests.
    ///
    /// Note: Due to borrowing constraints dispatch thunk is now propagated using DTH
    ///
    /// Returns uninitialized sandboxed module instance or an instantiation error.
    pub fn instantiate(
        &mut self,
        version: Instantiate,
        wasm: &[u8],
        guest_env: GuestEnvironment,
        sandbox_context: &mut dyn SandboxContext,
    ) -> std::result::Result<UnregisteredInstance, InstantiationError> {
        let context = &mut *RefCell::borrow_mut(&self.backend_context);
        let sandbox_instance = match context {
            BackendContext::Wasmi => wasmi_instantiate(wasm, guest_env, sandbox_context)?,

            BackendContext::Wasmer(context) => {
                wasmer_instantiate(version, context, wasm, guest_env, sandbox_context)?
            }
        };

        Ok(UnregisteredInstance { sandbox_instance })
    }

    /// Returns the backend context.
    pub fn backend_context(&self) -> Rc<RefCell<BackendContext>> {
        self.backend_context.clone()
    }
}

// Private routines
impl<DT> Store<DT> {
    fn register_sandbox_instance(
        &mut self,
        sandbox_instance: SandboxInstance,
        dispatch_thunk: DT,
    ) -> u32 {
        let instance_idx = self.instances.len();
        self.instances
            .push(Some((Rc::pin(sandbox_instance), dispatch_thunk)));
        instance_idx as u32
    }
}
