use codec::{Encode, Decode};

/// Program identifier.
///
/// 256-bit program identifier. In production environments, should be the result of a cryptohash function.
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
    /// Create new program id from bytes.
    ///
    /// Will panic if slice is not 32 bytes length.
    pub fn from_slice(s: &[u8]) -> Self {
        assert_eq!(s.len(), 32);
        let mut id = ProgramId([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    /// Return reference to raw bytes of this program id.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    /// Return mutable reference to raw bytes of this program id.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

/// Program.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: Vec<u8>,
    // Saved state of static pages
    static_pages: Vec<u8>,
}

impl Program {
    /// New program with speicif `id`, `code` and `static_pages`.
    pub fn new(id: ProgramId, code: Vec<u8>, static_pages: Vec<u8>) -> Self {
        Program {
            id,
            code,
            static_pages,
        }
    }

    /// Reference to code of this program.
    pub fn code(&self) -> &[u8] {
        &self.code[..]
    }

    /// Get the id of this program.
    pub fn id(&self) -> ProgramId {
        self.id
    }

    /// Reference to static area memory of this program.
    pub fn static_pages(&self) -> &[u8] {
        &self.static_pages
    }

    /// Mutable reference to static area memory of this program.
    pub fn static_pages_mut(&mut self) -> &mut Vec<u8> {
        &mut self.static_pages
    }

    /// Set the fcode of this program.
    pub fn set_code(&mut self, code: Vec<u8>) {
        self.code = code;
    }

    /// Clear static are of this program.
    pub fn clear_static(&mut self) {
        self.static_pages = vec![];
    }
}
