use wasmtime::{Store, Module, Func, Extern, Instance, Memory as WasmMemory};
use codec::{Encode, Decode};
use anyhow::{anyhow, Result};

use crate::{
    memory::{Allocations, PageNumber, MemoryContext},
    message::{Message, IncomingMessage, OutgoingMessage, MessageContext},
    program::{ProgramId, Program},
    storage::{AllocationStorage, ProgramStorage, MessageQueue, Storage},
};

#[derive(Clone, Debug, Decode, Encode)]
pub struct Config {
    static_pages: PageNumber,
    max_pages: PageNumber,
}

impl Default for Config {
    fn default() -> Self {
        Self { static_pages: BASIC_PAGES.into(), max_pages: MAX_PAGES.into() }
    }
}

pub struct Runner<AS: AllocationStorage, MQ: MessageQueue, PS: ProgramStorage> {
    pub(crate) program_storage: PS,
    pub(crate) message_queue: MQ,
    pub(crate) store: Store,
    pub(crate) memory: WasmMemory,
    pub(crate) allocations: Allocations<AS>,
    pub(crate) config: Config,
}

impl<AS: AllocationStorage + 'static, MQ: MessageQueue, PS: ProgramStorage> Runner<AS, MQ, PS> {
    pub fn new(
        config: &Config,
        storage: Storage<AS, MQ, PS>,
        persistent_memory: &[u8],
    ) -> Self {
        let store = Store::default();

        // memory need to be at least static_pages + persistent_memory length (in pages)
        let persistent_pages = persistent_memory.len() / BASIC_PAGE_SIZE;
        let total_pages = config.static_pages.raw() + persistent_pages as u32;

        let memory =
            WasmMemory::new(
                &store,
                wasmtime::MemoryType::new(
                    wasmtime::Limits::at_least(total_pages)
                ),
            );

        let persistent_region_start = config.static_pages.raw() as usize * BASIC_PAGE_SIZE;
        let persistent_region_end = persistent_region_start + persistent_memory.len();

        unsafe {
            memory
                .data_unchecked_mut()[persistent_region_start..persistent_region_end]
                .copy_from_slice(persistent_memory);
        }

        let Storage { allocation_storage, message_queue, program_storage } = storage;

        Self {
            program_storage,
            message_queue,
            memory,
            store,
            allocations: Allocations::new(allocation_storage),
            config: config.clone(),
        }
    }

    pub fn run_next(&mut self) -> Result<usize> {
        let next_message = match self.message_queue.dequeue() {
            Some(msg) => msg,
            None => { return Ok(0); }
        };

        if next_message.dest() == 0.into() {
            match String::from_utf8(next_message.payload().to_vec()) {
                Ok(s) => println!("UTF-8 msg to /0: {}", s),
                Err(_) => {
                    println!("msg to /0: {:?}", next_message.payload())
                }
            }
            Ok(1)
        } else {
            let mut context = self.create_context();
            let program = self.program_storage.get_mut(next_message.dest()).expect("Program not found");

            run(&mut context, program, EntryPoint::Handle, &next_message.into())?;
            self.message_queue.queue_many(context.message_buf.drain(..).collect());
            Ok(1)
        }
    }

    pub fn complete(self) -> (Storage<AS, MQ, PS>, Vec<u8>) {
        let persistent_memory = {
            let non_static_region_start = self.static_pages().raw() as usize * BASIC_PAGE_SIZE;
            unsafe { &self.memory.data_unchecked()[non_static_region_start..] }.to_vec()
        };

        let Runner { program_storage, message_queue, allocations, .. } = self;

        let allocation_storage = match allocations.drain() {
            Ok(v) => v,
            Err(e) => {
                panic!("Panic finalizing allocations: {:?}", e)
            }
        };

        (
            Storage {
                allocation_storage,
                message_queue,
                program_storage,
            },
            persistent_memory,
        )
    }

    pub fn static_pages(&self) -> PageNumber {
        self.config.static_pages
    }

    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    pub fn create_context(&self) -> RunningContext<AS> {
        RunningContext::new(
            &self.config,
            self.store.clone(),
            self.memory.clone(),
            self.allocations.clone(),
        )
    }

    pub fn init_program(&mut self, program_id: ProgramId, code: Vec<u8>, init_msg: Vec<u8>)
        -> Result<()>
    {
        if let Some(program) = self.program_storage.get_mut(program_id) {
            program.set_code(code.to_vec());
            program.clear_static();
        } else {
            self.program_storage.set(Program::new(program_id, code, vec![]));
        }

        self.allocations.clear(program_id);

        let mut context = self.create_context();
        let program = self.program_storage.get_mut(program_id).expect("Added above; cannot fail");
        let msg = IncomingMessage::new_system(init_msg.into());
        run(&mut context, program, EntryPoint::Init, &msg)?;
        self.message_queue.queue_many(context.message_buf.drain(..).collect());
        Ok(())
    }

    pub fn queue_message(&mut self, destination: ProgramId, payload: Vec<u8>) {
        self.message_queue.queue(Message::new_system(destination, payload.into()))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum EntryPoint {
    Handle,
    Init,
}

impl Into<&'static str> for EntryPoint {
    fn into(self) -> &'static str {
        match self {
            Self::Handle => "handle",
            Self::Init => "init",
        }
    }
}

static BASIC_PAGES: u32 = 256;
static BASIC_PAGE_SIZE: usize = 65536;
static MAX_PAGES: u32 = 16384;

pub struct RunningContext<AS: AllocationStorage> {
    config: Config,
    store: Store,
    memory: WasmMemory,
    allocations: Allocations<AS>,
    message_buf: Vec<Message>,
}

impl<AS: AllocationStorage> RunningContext<AS> {
    pub fn new(
        config: &Config,
        store: Store,
        memory: WasmMemory,
        allocations: Allocations<AS>,
    ) -> Self {
        Self {
            config: config.clone(),
            message_buf: vec![],
            store,
            memory,
            allocations,
        }
    }

    pub fn wasmtime_memory(&self) -> wasmtime::Memory {
        self.memory.clone()
    }

    pub fn static_pages(&self) -> PageNumber {
        self.config.static_pages
    }

    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    pub fn push_message(&mut self, msg: Message) {
        self.message_buf.push(msg)
    }
}

#[derive(Clone, Debug, Decode, Default, Encode, derive_more::From)]
pub struct RunResult {
    allocations: Vec<PageNumber>,
    touched: Vec<PageNumber>,
    messages: Vec<OutgoingMessage>,
}

pub fn run<AS: AllocationStorage + 'static>(
    context: &mut RunningContext<AS>,
    program: &mut Program,
    entry_point: EntryPoint,
    message: &IncomingMessage,
) -> Result<RunResult> {
    let module = Module::new(context.store.engine(), program.code())?;
    let memory = context.wasmtime_memory();
    let messages = MessageContext::new(
        program.id(),
        message.clone(),
    );

    let memory_context = MemoryContext::new(
        program.id(),
        Box::new(memory.clone()),
        context.allocations.clone(),
        context.static_pages(),
        context.max_pages(),
    );

    let mut imports = module
        .imports()
        .map(
            |import| if import.module() != "env" {
                return Err(anyhow!("Non-env imports are not supported"))
            } else {
                Ok((import.name(), Option::<Extern>::None))
            }
        )
        .collect::<anyhow::Result<Vec<_>>>()?;

    for (ref import_name, ref mut ext) in imports.iter_mut() {
        let func = if import_name == &"send" {
            let memory_clone = memory_context.memory().clone();
            let messages_clone = messages.clone();
            Func::wrap(
                &context.store,
                move |program_id: i64, message_ptr: i32, message_len: i32| {
                    let message_ptr = message_ptr as u32 as usize;
                    let message_len = message_len as u32 as usize;
                    let data = unsafe { &memory_clone.data_unchecked()[message_ptr..message_ptr+message_len] };
                    if let Err(_) = messages_clone.send({
                        println!("outgoing msg: {:?}", data);
                        OutgoingMessage::new(ProgramId(program_id as _), data.to_vec().into())
                    }) {
                        return Err(wasmtime::Trap::new("Trapping: unable to send message"));
                    }

                    Ok(())
                },
            )
        } else if import_name == &"source" {
            let id = message.source().map(|x| x.0).unwrap_or_default() as i64;
            Func::wrap(&context.store, move || {
                Ok(id)
            })
        } else if import_name == &"alloc" {
            let mem_ctx = memory_context.clone();
            Func::wrap(&context.store, move |pages: i32| {
                let pages = pages as u32;
                let ptr = match mem_ctx.alloc(pages.into()) {
                    Ok(ptr) => ptr.raw(),
                    _ => { return Ok(0u32); }
                };

                println!("ALLOC: {} pages at {}", pages, ptr);

                Ok(ptr)
            })
        } else if import_name == &"free" {
            let mem_ctx = memory_context.clone();
            Func::wrap(&context.store, move |page: i32| {
                let page = page as u32;
                if let Err(e) = mem_ctx.free(page.into()) {
                    println!("FREE ERROR: {:?}", e);
                } else {
                    println!("FREE: {}", page);
                }
                Ok(())
            })
        } else if import_name == &"size" {
            let messages_clone = messages.clone();
            Func::wrap(&context.store, move || Ok(messages_clone.current().payload().len() as u32 as i32))
        } else if import_name == &"read" {
            let messages_clone = messages.clone();
            let memory_clone = memory_context.memory().clone();
            Func::wrap(&context.store, move |at: i32, len: i32, dest: i32| {
                let at = at as u32 as usize;
                let len = len as u32 as usize;
                let dest = dest as u32 as usize;
                let message_data = &messages_clone.current().payload()[at..at+len];
                unsafe { memory_clone.data_unchecked_mut()[dest..dest+len].copy_from_slice(message_data); }
                Ok(())
            })
        } else if import_name == &"debug" {
            let memory_clone = memory_context.memory().clone();
            Func::wrap(
                &context.store,
                move |str_ptr: i32, str_len: i32| {
                    let str_ptr = str_ptr as u32 as usize;
                    let str_len = str_len as u32 as usize;
                    let debug_str = unsafe {
                        String::from_utf8_unchecked(
                           memory_clone.data_unchecked()[str_ptr..str_ptr+str_len].to_vec()
                        )
                    };
                    println!("DEBUG: {}", debug_str);

                    Ok(())
                },
            )
        } else if import_name == &"memory" {
            *ext = Some(memory.clone().into());
            continue;
        } else {
            continue;
        };

        *ext = Some(func.into());
    }

    let externs = imports
        .into_iter()
        .map(|(_, host_function)| host_function.ok_or(anyhow!("Missing import")))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let instance = Instance::new(
        &context.store,
        &module,
        &externs,
    )?;

    // Set static pages from saved program state.
    unsafe {
        let cut_off = program.static_pages().len();
        memory_context.memory().data_unchecked_mut()[0..cut_off]
            .copy_from_slice(program.static_pages());
    };

    let handler = instance
        .get_func(entry_point.into())
        .ok_or(
            anyhow::format_err!("failed to find `{}` function export", Into::<&'static str>::into(entry_point))
        )?
        .get0::<()>()?;

    handler()?;

    // Save program static pages.
    *program.static_pages_mut() = unsafe {
        memory_context.memory().data_unchecked()[0..context.static_pages().raw() as usize * BASIC_PAGE_SIZE].to_vec()
    };

    for outgoing_msg in messages.drain() {
        context.push_message(outgoing_msg.into_message(program.id()));
    }

    Ok(RunResult::default())
}
