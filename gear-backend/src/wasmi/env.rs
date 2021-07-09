//! Wasmi environment for running a module.

use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, MemoryDescriptor,
    MemoryRef, ModuleImportResolver, ModuleInstance, ModuleRef, RuntimeArgs, RuntimeValue,
    Signature, Trap, TrapKind, ValueType,
};

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use ::anyhow::{self};

use super::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt, PageAction, PageInfo};
use gear_core::memory::{Memory, PageNumber};
use gear_core::message::OutgoingMessage;
use gear_core::program::ProgramId;

/// Struct for implementing host functions
///
/// contains host func indices inherited from Environment
struct Runtime<E: Ext + 'static> {
    ext: LaterExt<E>,
    send: usize,
    source: usize,
    alloc: usize,
    free: usize,
    size: usize,
    read: usize,
    debug: usize,
    gas: usize,
    value: usize,
}

impl<E: Ext + 'static> Externals for Runtime<E> {
    /// host functions implementations
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
        match index {
            id if id == self.alloc => {
                let ext = self.ext.clone();
                let pages: u32 = args.nth(0);
                let ptr = match ext.with(|ext: &mut E| ext.alloc(pages.into())) {
                    Ok(ptr) => ptr.raw(),
                    _ => {
                        return Ok(Some(RuntimeValue::I32(0)));
                    }
                };

                log::debug!("ALLOC: {} pages at {}", pages, ptr);

                Ok(Some(RuntimeValue::I32(ptr as i32)))
            }
            id if id == self.send => {
                let ext = self.ext.clone();
                let program_id_ptr: i32 = args.nth(0);
                let message_ptr: i32 = args.nth(1);
                let message_len: i32 = args.nth(2);
                let gas_limit: i64 = args.nth(3);
                let value_ptr: i32 = args.nth(4);
                let message_ptr = message_ptr as u32 as usize;
                let message_len = message_len as u32 as usize;
                if ext
                    .with(|ext: &mut E| {
                        let mut data = vec![0u8; message_len];
                        ext.get_mem(message_ptr, &mut data);
                        let mut program_id = [0u8; 32];
                        ext.get_mem(program_id_ptr as isize as _, &mut program_id);
                        let program_id = ProgramId::from_slice(&program_id);

                        let mut value_le = [0u8; 16];
                        ext.get_mem(value_ptr as isize as _, &mut value_le);

                        ext.send(OutgoingMessage::new(
                            program_id,
                            data.into(),
                            gas_limit as _,
                            u128::from_le_bytes(value_le),
                        ))
                    })
                    .is_err()
                {
                    return Err(Trap::new(TrapKind::UnexpectedSignature));
                }

                Ok(None)
            }
            id if id == self.free => {
                let ext = self.ext.clone();
                let page: i32 = args.nth(0);
                let page = page as u32;
                if let Err(e) = ext.with(|ext: &mut E| ext.free(page.into())) {
                    log::debug!("FREE ERROR: {:?}", e);
                } else {
                    log::debug!("FREE: {}", page);
                }
                Ok(None)
            }
            id if id == self.size => {
                let ext = self.ext.clone();
                ext.with(|ext: &mut E| Ok(Some(RuntimeValue::I32(ext.msg().len() as i32))))
            }
            id if id == self.read => {
                let ext = self.ext.clone();
                let at = args.nth::<i32>(0) as usize;
                let len = args.nth::<i32>(1) as usize;
                let dest = args.nth::<i32>(2) as usize;
                ext.with(|ext: &mut E| {
                    let msg = ext.msg().to_vec();
                    ext.set_mem(dest, &msg[at..at + len]);
                });
                Ok(None)
            }
            id if id == self.debug => {
                let ext = self.ext.clone();
                let str_ptr = args.nth::<i32>(0) as usize;
                let str_len = args.nth::<i32>(1) as usize;
                ext.with(|ext: &mut E| {
                    let mut data = vec![0u8; str_len];
                    ext.get_mem(str_ptr, &mut data);
                    let debug_str = unsafe { String::from_utf8_unchecked(data) };
                    log::debug!("DEBUG: {}", debug_str);
                });
                Ok(None)
            }
            id if id == self.source => {
                let ext = self.ext.clone();
                let source_ptr = args.nth::<i32>(0) as usize;
                ext.with(|ext: &mut E| {
                    let source = ext.source();
                    ext.set_mem(source_ptr as isize as _, source.as_slice());
                });
                Ok(None)
            }
            id if id == self.gas => {
                let ext = self.ext.clone();
                let val = args.nth::<i32>(0);
                if ext.with(|ext: &mut E| ext.gas(val as _)).is_err() {
                    Err(wasmi::Trap::new(TrapKind::InvalidConversionToInt))
                } else {
                    Ok(None)
                }
            }
            id if id == self.value => {
                let ext = self.ext.clone();
                let value_ptr = args.nth::<i32>(0);
                ext.with(|ext: &mut E| {
                    let source = ext.value();
                    ext.set_mem(value_ptr as isize as _, &source.to_le_bytes()[..]);
                });
                Ok(None)
            }
            _ => panic!("unknown function index"),
        }
    }
}

impl<E: Ext + 'static> ModuleImportResolver for Environment<E> {
    /// Provide imports corresponding concrete reference
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &Signature,
    ) -> Result<FuncRef, InterpreterError> {
        let func_ref = match field_name {
            "alloc" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32][..], Some(ValueType::I32)),
                self.alloc,
            ),
            "gr_send" => FuncInstance::alloc_host(
                Signature::new(
                    &[
                        ValueType::I32,
                        ValueType::I32,
                        ValueType::I32,
                        ValueType::I64,
                        ValueType::I32,
                    ][..],
                    None,
                ),
                self.send,
            ),
            "free" => {
                FuncInstance::alloc_host(Signature::new(&[ValueType::I32][..], None), self.free)
            }
            "gr_size" => {
                FuncInstance::alloc_host(Signature::new(&[][..], Some(ValueType::I32)), self.size)
            }
            "gr_read" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32, ValueType::I32, ValueType::I32][..], None),
                self.read,
            ),
            "gr_debug" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32, ValueType::I32][..], None),
                self.debug,
            ),
            "gr_source" => {
                FuncInstance::alloc_host(Signature::new(&[ValueType::I32][..], None), self.source)
            }
            "gas" => {
                FuncInstance::alloc_host(Signature::new(&[ValueType::I32][..], None), self.gas)
            }
            "gr_value" => {
                FuncInstance::alloc_host(Signature::new(&[ValueType::I32][..], None), self.value)
            }
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
    send: usize,
    source: usize,
    alloc: usize,
    free: usize,
    size: usize,
    read: usize,
    debug: usize,
    gas: usize,
    value: usize,
}

impl<E: Ext + 'static> Environment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        let ext = LaterExt::new();

        // host func index
        let alloc = 1;
        let send = 2;
        let free = 3;
        let size = 4;
        let read = 5;
        let debug = 6;
        let source = 7;
        let gas = 8;
        let value = 9;

        Self {
            ext,
            memory: None,
            alloc,
            send,
            free,
            size,
            read,
            debug,
            source,
            gas,
            value,
        }
    }

    fn run_inner(
        &mut self,
        module: ModuleRef,
        static_area: Vec<u8>,
        memory: &dyn Memory,
        func: impl FnOnce(ModuleRef) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        memory.write(0, &static_area).expect("Err write mem");

        func(module)
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
        static_area: Vec<u8>,
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
            alloc: self.alloc,
            send: self.send,
            free: self.free,
            size: self.size,
            read: self.read,
            debug: self.debug,
            source: self.source,
            gas: self.gas,
            value: self.value,
        };

        let result = self.run_inner(instance, static_area, memory, move |instance| {
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
}
