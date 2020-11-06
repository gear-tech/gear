mod memory;

use wasmtime::{Store, Module, Func, Extern, Instance};
use codec::{Encode, Decode};
use anyhow::anyhow;

static BASIC_PAGES: usize = 256;
static BASIC_PAGE_SIZE: usize = 65536;
static BASIC_TOTAL_SIZE: usize = BASIC_PAGES * BASIC_PAGE_SIZE;
static MAX_PAGES: usize = 16384;

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq)]
pub struct ProgramId(u64);

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Payload(Vec<u8>);

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageNumber(u32);

impl PageNumber {
    fn raw(&self) -> u32 { self.0 }
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Code(Vec<u8>);

#[derive(Clone, Debug, Decode, Encode)]
pub struct Allocation {
    program_id: ProgramId,
    page_id: PageNumber,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: Code,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct Context {
    static_pages: PageNumber,
    cut_off: PageNumber,
    memory: Memory,
}

pub struct RunningContext {
    store: Store,
    context: Context,
}

impl RunningContext {
    pub fn wasmtime_memory(&self) -> wasmtime::Memory {
        wasmtime::Memory::new(
            &self.store,
            wasmtime::MemoryType::new(
                wasmtime::Limits::at_least(self.context.cut_off.0 as _)
            ),
        )
    }
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Memory {
    data: Vec<u8>,
    allocated: PageNumber,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Message {
    source: Option<ProgramId>,
    program_id: ProgramId,
    payload: Payload,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct IncomingMessage {
    source: Option<ProgramId>,
    payload: Payload,
}

impl IncomingMessage {
    fn empty() -> Self {
        Self {
            source: None,
            payload: Payload(vec![]),
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct OutgoingMessage {
    destination: ProgramId,
    payload: Payload,
}

#[derive(Clone, Debug, Decode, Default, Encode, derive_more::From)]
pub struct RunResult {
    allocations: Vec<PageNumber>,
    touched: Vec<PageNumber>,
    messages: Vec<OutgoingMessage>,
}

pub fn run(
    context: &mut RunningContext,
    program: &Program,
    message: &IncomingMessage,
) -> anyhow::Result<RunResult> {
    use std::{rc::Rc, cell::RefCell};

    let module = Module::new(context.store.engine(), &program.code.0[..])?;
    let memory = context.wasmtime_memory();
    let allocations = Rc::new(RefCell::new(vec![]));
    let messages = Rc::new(RefCell::new(vec![]));
    let incoming_message = Rc::new(RefCell::new(message.clone()));

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
            let memory_clone = memory.clone();
            let messages_clone = messages.clone();
            Func::wrap(
                &context.store,
                move |program_id: i64, message_ptr: i32, message_len: i32| {
                    let message_ptr = message_ptr as u32 as usize;
                    let message_len = message_len as u32 as usize;
                    let data = unsafe { &memory_clone.data_unchecked()[message_ptr..message_ptr+message_len] };
                    messages_clone.borrow_mut().push(
                        OutgoingMessage {
                            destination: ProgramId(program_id as _),
                            payload: data.to_vec().into(),
                        }
                    );

                    Ok(())
                },
            )
        } else if import_name == &"alloc" {
            let memory_clone = memory.clone();
            let allocations_clone = allocations.clone();
            let start = context.context.cut_off.0;
            Func::wrap(&context.store, move |pages: i32| {
                let pages = pages as u32;
                let ptr = memory_clone.grow(pages)?;

                for page in ptr+1..ptr+1+pages {
                    allocations_clone.borrow_mut().push(start + page);
                    println!("ALLOC: {}", page);
                }

                Ok(ptr)
            })
        } else if import_name == &"free" {
            let allocations_clone = allocations.clone();
            Func::wrap(&context.store, move |page: i32| {
                let page = page as u32;
                allocations_clone.borrow_mut().retain(|p| *p != page);

                println!("FREE: {}", page);
                Ok(())
            })
        } else if import_name == &"size" {
            let message_clone = incoming_message.clone();
            Func::wrap(&context.store, move || Ok(message_clone.borrow().payload.0.len() as u32 as i32))
        } else if import_name == &"read" {
            let message_clone = incoming_message.clone();
            let memory_clone = memory.clone();
            Func::wrap(&context.store, move |at: i32, len: i32, dest: i32| {
                let incoming_message = message_clone.borrow();
                let at = at as u32 as usize;
                let len = len as u32 as usize;
                let dest = dest as u32 as usize;
                let message_data = &incoming_message.payload.0[at..at+len];
                unsafe { memory_clone.data_unchecked_mut()[dest..dest+len].copy_from_slice(message_data); }
                Ok(())
            })
        } else if import_name == &"debug" {
            let memory_clone = memory.clone();
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
            let memory_clone = memory.clone();
            *ext = Some(memory_clone.into());
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

    let handler = instance
        .get_func("handle")
        .ok_or(anyhow::format_err!("failed to find `handle` function export"))?
        .get0::<()>()?;

    handler()?;

    Ok(RunResult::default())
}

pub fn running_context() -> RunningContext {
    RunningContext {
        store: Store::default(),
        context: Context {
            static_pages: 1.into(),
            cut_off: (BASIC_PAGES as u32).into(),
            memory: Memory {
                data: {
                    let mut v = Vec::with_capacity(BASIC_TOTAL_SIZE);
                    v.resize(BASIC_TOTAL_SIZE, 0);
                    v
                },
                allocated: 0.into(),
            },
        }
    }
}

fn main() -> Result<(), anyhow::Error> {
    let file_name = std::env::args().nth(1).expect("gear <filename.wasm>");
    let mut context = running_context();
    let program = Program {
        id: 1.into(),
        code: std::fs::read(file_name)?.into(),
    };

    run(&mut context, &program, &IncomingMessage::empty())?;

    Ok(())
}
