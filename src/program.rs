use codec::{Encode, Decode};

#[derive(Clone, Copy, Debug, Decode, Default, Encode, derive_more::From, Hash, PartialEq, Eq)]
pub struct ProgramId([u8; 32]);

impl From<u64> for ProgramId {
    fn from(v: u64) -> Self {
        let mut id = ProgramId([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
    }
}

impl ProgramId {
    pub fn from_slice(s: &[u8]) -> Self {
        assert_eq!(s.len(), 32);
        let mut id = ProgramId([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
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

    pub fn clear_static(&mut self) {
        self.static_pages = vec![];
    }
}
