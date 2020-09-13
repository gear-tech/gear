
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
    running_pages: PageNumber,
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

pub struct SpawnedMessage {
    destination: ProgramId,
    payload: Payload,
}

fn main() {
    
}
