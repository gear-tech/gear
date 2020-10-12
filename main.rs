use wasmtime::{Store, Module, Func};
use codec::{Encode, Decode};

static BASIC_PAGES: usize = 16;
static BASIC_PAGE_SIZE: usize = 65536;
static BASIC_TOTAL_SIZE: usize = BASIC_PAGES * BASIC_PAGE_SIZE;
static MAX_PAGES: usize = 16384;

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From)]
pub struct ProgramId(u64);

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Payload(Vec<u8>);

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From)]
pub struct PageNumber(u32);

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Code(Vec<u8>);

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Allocation {
    program_id: ProgramId,
    page_id: PageNumber,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Program {
    id: ProgramId,
    code: Code,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
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

    let module = Module::new(context.store.engine(), &program.code.0[..]);
    let memory = context.wasmtime_memory();
    let allocations = Rc::new(RefCell::new(vec![]));
    let messages = Rc::new(RefCell::new(vec![]));

    let memory_clone = memory.clone();
    let allocations_clone = allocations.clone();
    let start = context.context.cut_off.0;
    let allocator = Func::wrap(&context.store, move |pages: i32| {
        let pages = pages as u32;
        let ptr = memory_clone.grow(pages)?;

        for page in 0..pages {
            allocations_clone.borrow_mut().push(start + page);
        }

        Ok(ptr)
    });

    let memory_clone = memory.clone();
    let messages_clone = messages.clone();
    let send_message = Func::wrap(
        &context.store,
        move |program_id: i64, message_ptr: i32, message_len: i32| {
            let message_ptr = message_ptr as u32 as usize;
            let message_len = message_ptr as u32 as usize;
            let data = unsafe { &memory_clone.data_unchecked()[message_ptr..message_ptr+message_len] };
            messages_clone.borrow_mut().push(
                OutgoingMessage {
                    destination: ProgramId(program_id as _),
                    payload: data.to_vec().into(),
                }
            );

            Ok(())
        },
    );

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
