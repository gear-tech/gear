use codec::{Encode, Decode};

#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From, Hash, PartialEq, Eq)]
pub struct ProgramId(pub u64);

#[derive(Clone, Debug)]
pub enum Error {
    InitializatingError(&'static str),
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: Vec<u8>,
    // Saved state of static pages
    static_pages: Vec<u8>,
}

impl Program {
    pub fn new(id: ProgramId, code: Vec<u8>, static_pages: Vec<u8>) -> Self {
        Program {
            id,
            code,
            static_pages,
        }
    }

    pub fn code(&self) -> &[u8] {
        &self.code[..]
    }

    pub fn id(&self) -> ProgramId {
        self.id
    }
}
