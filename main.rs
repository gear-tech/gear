
pub struct ProgramId(u64);

pub struct Payload(Vec<u8>);

pub struct PageNumber(usize);

pub struct Code(Vec<u8>);

pub struct Allocation {
    program_id: ProgramId,
    page_id: PageNumber,
}

pub struct Program {
    id: ProgramId,
    code: Code,
}

pub struct Context {
    static_pages: PageNumber,
    cut_off: PageNumber,
    memory: Memory,
}

pub struct Memory {
    data: Vec<u8>,
    allocated: PageNumber,
}

pub struct Message {
    source: ProgramId,
    program_id: ProgramId,
    payload: Payload,
}

pub struct IncomingMessage {
    source: ProgramId,
    payload: Payload,
}

pub struct OutgoingMessage {
    destination: ProgramId,
    payload: Payload,
}

#[derive(Default)]
pub struct RunResult {
    allocations: Vec<PageNumber>,
    touched: Vec<PageNumber>,
    messages: Vec<OutgoingMessage>,
}

pub fn run(context: &mut Context, program: &Program, messages: &[Message]) -> Result<RunResult, &'static str> {
    Ok(RunResult::default())
}

fn main() {

}
