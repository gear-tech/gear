use wasmtime::{Config, Engine};
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

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Memory {
    data: Vec<u8>,
    allocated: PageNumber,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Message {
    source: ProgramId,
    program_id: ProgramId,
    payload: Payload,
}

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct IncomingMessage {
    source: ProgramId,
    payload: Payload,
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

pub fn run(context: &mut Context, program: &Program, message: &Message) -> Result<RunResult, &'static str> {
    Ok(RunResult::default())
}

fn main() {

    let file_name = std::env::args().nth(1).expect("wfork <filename.wasm>");

    let config = Config::default();
    let engine = Engine::new(&config);

}
