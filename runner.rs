use std::collections::{HashMap, VecDeque, hash_map::Entry};

use wasmtime::{Store, Module, Func, Extern, Instance};
use codec::{Encode, Decode};
use anyhow::{anyhow, Result};

use crate::{
    memory::{Allocations, PageNumber, MemoryContext},
    message::{Message, IncomingMessage, Payload, OutgoingMessage, MessageContext},
    program::{ProgramId, Program},
};

pub struct Runner {
    pub(crate) programs: HashMap<ProgramId, Program>,
    pub(crate) message_queue: VecDeque<Message>,
    pub(crate) context: RunningContext,
}

pub struct Output(Vec<u8>);

impl Runner {
    pub fn new(
        programs: Vec<Program>,
        allocations: Vec<(PageNumber, ProgramId)>,
        message_queue: Vec<Message>,
        memory: &[u8],
    ) -> Self {
        Self {
            programs: programs.into_iter().map(|p| (p.id(), p)).collect(),
            message_queue: VecDeque::from(message_queue),
            context: RunningContext::new(
                &Config::default(),
                Store::default(),
                memory,
                Allocations::new(allocations),
            ),
        }
    }

    pub fn run_next(&mut self) -> Result<Vec<Output>> {
        let next_message = match self.message_queue.pop_front() {
            Some(msg) => msg,
            None => { return Ok(vec![]); }
        };

        let program = self.programs.get_mut(&next_message.dest).expect("Program not found");

        run(&mut self.context, program, &next_message.into()).map(|_| vec![])
    }

    pub fn complete(self) -> (
        Vec<Program>,
        Vec<(PageNumber, ProgramId)>,
        Vec<Message>,
        Vec<u8>,
    ) {
        let Runner { mut programs, context, message_queue } = self;
        (
            programs.drain().map(|(_, v)| v).collect(),
            context.allocations().clone().drain(),
            message_queue.into_iter().collect(),
            context.copy_memory(),
        )
    }

    pub fn update_program_code(&mut self, program_id: ProgramId, code: Vec<u8>) {
        self.programs.entry(program_id)
            .and_modify(|v| v.set_code(code.to_vec()))
            .or_insert_with(|| Program::new(program_id, code, vec![]));
    }
}

static BASIC_PAGES: u32 = 256;
static BASIC_PAGE_SIZE: usize = 65536;
static BASIC_TOTAL_SIZE: usize = BASIC_PAGES as usize * BASIC_PAGE_SIZE;
static MAX_PAGES: u32 = 16384;

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

pub struct RunningContext {
    config: Config,
    store: Store,
    memory: wasmtime::Memory,
    allocations: Allocations,
}

impl RunningContext {
    pub fn basic(persistent_memory: &[u8]) -> Self {
        Self::new(
            &Config::default(),
            Store::default(),
            persistent_memory,
            Allocations::default(),
        )
    }

    pub fn new(
        config: &Config,
        store: Store,
        persistent_memory: &[u8],
        allocations: Allocations,
    ) -> Self {
        // memory need to be at least static_pages + persistent_memory length (in pages)
        let persistent_pages = persistent_memory.len() / BASIC_PAGE_SIZE;
        let total_pages = config.static_pages.raw() + persistent_pages as u32;

        let memory =
            wasmtime::Memory::new(
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

        Self {
            config: config.clone(),
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

    pub fn allocations(&self) -> &Allocations {
        &self.allocations
    }

    pub fn copy_memory(&self) -> Vec<u8> {
        let non_static_region_start = self.static_pages().raw() as usize * BASIC_PAGE_SIZE;

        unsafe { &self.memory.data_unchecked()[non_static_region_start..] }.to_vec()
    }
}

#[derive(Clone, Debug, Decode, Default, Encode, derive_more::From)]
pub struct RunResult {
    allocations: Vec<PageNumber>,
    touched: Vec<PageNumber>,
    messages: Vec<OutgoingMessage>,
}

pub fn run(
    context: &mut RunningContext,
    program: &mut Program,
    message: &IncomingMessage,
) -> Result<RunResult> {
    let module = Module::new(context.store.engine(), program.code())?;
    let memory = context.wasmtime_memory();
    let messages = MessageContext::new(program.id(), message.clone());

    let memory_context = MemoryContext::new(
        program.id(),
        memory,
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
            let memory_clone = memory_context.wasm().clone();
            let messages_clone = messages.clone();
            Func::wrap(
                &context.store,
                move |program_id: i64, message_ptr: i32, message_len: i32| {
                    let message_ptr = message_ptr as u32 as usize;
                    let message_len = message_len as u32 as usize;
                    let data = unsafe { &memory_clone.data_unchecked()[message_ptr..message_ptr+message_len] };
                    if let Err(_) = messages_clone.send(
                        OutgoingMessage::new(ProgramId(program_id as _), data.to_vec().into())
                    ) {
                        return Err(wasmtime::Trap::new("Trapping: unable to send message"));
                    }

                    Ok(())
                },
            )
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
            let memory_clone = memory_context.wasm().clone();
            Func::wrap(&context.store, move |at: i32, len: i32, dest: i32| {
                let at = at as u32 as usize;
                let len = len as u32 as usize;
                let dest = dest as u32 as usize;
                let message_data = &messages_clone.current().payload()[at..at+len];
                unsafe { memory_clone.data_unchecked_mut()[dest..dest+len].copy_from_slice(message_data); }
                Ok(())
            })
        } else if import_name == &"debug" {
            let memory_clone = memory_context.wasm().clone();
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
            *ext = Some(memory_context.wasm().clone().into());
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
        memory_context.wasm().data_unchecked_mut()[0..cut_off]
            .copy_from_slice(program.static_pages());
    };

    let handler = instance
        .get_func("handle")
        .ok_or(anyhow::format_err!("failed to find `handle` function export"))?
        .get0::<()>()?;

    handler()?;

    // Save program static pages.
    *program.static_pages_mut() = unsafe {
        memory_context.wasm().data_unchecked()[0..context.static_pages().raw() as usize * BASIC_PAGE_SIZE].to_vec()
    };

    Ok(RunResult::default())
}
