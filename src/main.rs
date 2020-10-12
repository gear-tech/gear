use wasmtime::{Store, Module};
use codec::{Encode, Decode};

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From)]
pub struct ProgramId(u64);

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Payload(Vec<u8>);

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From)]
pub struct PageNumber(u64);

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
) -> Result<RunResult, &'static str> {
    let module = Module::new(context.store.engine(), &program.code.0[..]);

    Ok(RunResult::default())
}

pub fn running_context() -> RunningContext {
    let BASIC_PAGES = 256;
    let BASIC_PAGE_SIZE = 65536;
    let BASIC_TOTAL_SIZE = BASIC_PAGES * BASIC_PAGE_SIZE;

    RunningContext {
        store: Store::default(),
        context: Context {
            static_pages: 1.into(),
            cut_off: (BASIC_PAGES as u64).into(),
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

    run(&mut context, &program, &IncomingMessage::empty());

    Ok(())
}
