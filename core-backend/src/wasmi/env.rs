//! Wasmi environment for running a module.

use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, MemoryDescriptor,
    MemoryRef, ModuleImportResolver, ModuleInstance, ModuleRef, RuntimeArgs, RuntimeValue,
    Signature, Trap, TrapKind, ValueType,
};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use super::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt, PageAction, PageInfo};
use gear_core::memory::{Memory, PageBuf, PageNumber};

use crate::funcs;

/// Struct for implementing host functions.
struct Runtime<E: Ext + 'static> {
    ext: LaterExt<E>,
}

#[derive(FromPrimitive)]
enum FuncIndex {
    Alloc = 1,
    Charge,
    Commit,
    Debug,
    Free,
    Gas,
    MsgId,
    Init,
    Push,
    Read,
    Reply,
    ReplyTo,
    Send,
    Size,
    Source,
    Value,
}

macro_rules! func_instance {
    ($idx:ident, $($param:expr $(,)?)* => $res:expr) => {
        FuncInstance::alloc_host(
            Signature::new(&[$($param,)*][..], $res),
            FuncIndex::$idx as usize,
        )
    };
}

impl<E: Ext + 'static> Externals for Runtime<E> {
    /// Host functions implementations.
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
        match FromPrimitive::from_usize(index) {
            Some(FuncIndex::Alloc) => funcs::alloc(self.ext.clone())(args.nth(0))
                .map(|v| Some(RuntimeValue::I32(v as i32)))
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Charge) => funcs::charge(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::InvalidConversionToInt)),

            Some(FuncIndex::Commit) => funcs::commit(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Debug) => funcs::debug(self.ext.clone())(args.nth(0), args.nth(1))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Free) => funcs::free(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Gas) => funcs::gas(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::InvalidConversionToInt)),

            Some(FuncIndex::MsgId) => funcs::msg_id(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Init) => funcs::init(self.ext.clone())(
                args.nth(0),
                args.nth(1),
                args.nth(2),
                args.nth(3),
                args.nth(4),
            )
            .map(|_| None)
            .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Push) => {
                funcs::push(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::Read) => {
                funcs::read(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::Reply) => {
                funcs::reply(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2), args.nth(3))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }
            Some(FuncIndex::ReplyTo) => {
                funcs::reply_to(self.ext.clone())(args.nth(0))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }
            Some(FuncIndex::Send) => funcs::send(self.ext.clone())(
                args.nth(0),
                args.nth(1),
                args.nth(2),
                args.nth(3),
                args.nth(4),
            )
            .map(|_| None)
            .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Size) => Ok(Some(RuntimeValue::I32(funcs::size(self.ext.clone())()))),

            Some(FuncIndex::Source) => funcs::source(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Value) => funcs::value(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            _ => panic!("unknown function index"),
        }
    }
}

impl<E: Ext + 'static> ModuleImportResolver for Environment<E> {
    /// Provide imports corresponding concrete reference.
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &Signature,
    ) -> Result<FuncRef, InterpreterError> {
        let func_ref = match field_name {
            "alloc" => func_instance!(Alloc, ValueType::I32 => Some(ValueType::I32)),
            "free" => func_instance!(Free, ValueType::I32 => None),
            "gas" => func_instance!(Gas, ValueType::I32 => None),
            "gr_charge" => func_instance!(Charge, ValueType::I64 => None),
            "gr_commit" => func_instance!(Commit, ValueType::I32 => None),
            "gr_debug" => func_instance!(Debug, ValueType::I32, ValueType::I32 => None),
            "gr_init" => func_instance!(Init, ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32 => Some(ValueType::I32)),
            "gr_msg_id" => func_instance!(MsgId, ValueType::I32 => None),
            "gr_push" => {
                func_instance!(Push, ValueType::I32, ValueType::I32, ValueType::I32 => None)
            }
            "gr_read" => {
                func_instance!(Read, ValueType::I32, ValueType::I32, ValueType::I32 => None)
            }
            "gr_reply" => func_instance!(Reply, ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32 => None),
            "gr_reply_to" => func_instance!(ReplyTo, ValueType::I32 => None),
            "gr_send" => func_instance!(Send, ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32 => None),
            "gr_size" => func_instance!(Size,  => Some(ValueType::I32)),
            "gr_source" => func_instance!(Source, ValueType::I32 => None),
            "gr_value" => func_instance!(Value, ValueType::I32 => None),

            _ => {
                return Err(InterpreterError::Function(format!(
                    "host module doesn't export function with name {}",
                    field_name
                )));
            }
        };
        Ok(func_ref)
    }

    /// Map module memory to host memory
    fn resolve_memory(
        &self,
        _field_name: &str,
        _memory_type: &MemoryDescriptor,
    ) -> Result<MemoryRef, InterpreterError> {
        let mem = match self.memory.as_ref() {
            Some(memory) => memory,
            None => panic!("Memory is None"),
        };

        let mem = match mem.as_any().downcast_ref::<MemoryRef>() {
            Some(memory) => memory,
            None => panic!("Memory is not wasmi::MemoryRef"),
        };

        Ok(mem.clone())
    }
}

/// Environment to run one module at a time providing Ext.
pub struct Environment<E: Ext + 'static> {
    ext: LaterExt<E>,
    memory: Option<Box<dyn Memory>>,
}

impl<E: Ext + 'static> Default for Environment<E> {
    fn default() -> Self {
        Environment::<E>::new()
    }
}

impl<E: Ext + 'static> Environment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        Self {
            ext: LaterExt::new(),
            memory: None,
        }
    }

    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    pub fn setup_and_run(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E, Vec<(PageNumber, PageAction)>) {
        self.memory = Some(memory.clone());

        let module = wasmi::Module::from_buffer(binary).expect("Error creating module");

        let mut imports = ImportsBuilder::new();
        imports.push_resolver("env", self);

        let instance = ModuleInstance::new(&module, &imports)
            .expect("failed to instantiate wasm module")
            .assert_no_start();

        let touched: Rc<RefCell<Vec<PageInfo>>> = Rc::new(RefCell::new(Vec::new()));

        self.ext.set(ext);
        let mut runtime = Runtime {
            ext: self.ext.clone(),
        };

        let result = self.run_inner(instance, memory_pages, memory, move |instance| {
            instance
                .invoke_export(entry_point, &[], &mut runtime)
                .map_err(|err| anyhow::format_err!("Failed export: {:?}", err))
                .map(|_| ())
        });

        let ext = self.ext.unset();

        let touched = touched.take().iter().map(|(a, b, _)| (*a, *b)).collect();

        (result, ext, touched)
    }

    /// Create memory inside this environment.
    pub fn create_memory(&self, total_pages: u32) -> MemoryWrap {
        MemoryWrap::new(
            wasmi::MemoryInstance::alloc(wasmi::memory_units::Pages(total_pages as usize), None)
                .expect("Create env memory fail"),
        )
    }

    fn run_inner(
        &mut self,
        module: ModuleRef,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        func: impl FnOnce(ModuleRef) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        // Set module memory.
        memory
            .set_pages(memory_pages)
            .map_err(|_| anyhow::anyhow!("Can't set module memory"))?;

        func(module)
    }
}
