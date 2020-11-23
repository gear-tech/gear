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

    pub fn static_pages(&mut self) -> &[u8] {
        &mut self.static_pages
    }

    pub fn static_pages_mut(&mut self) -> &mut Vec<u8> {
        &mut self.static_pages
    }

    pub fn set_code(&mut self, code: Vec<u8>) {
        self.code = code;
    }
}
