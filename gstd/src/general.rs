pub struct MessageHandle(pub u32);

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct MessageId(pub [u8; 32]);

impl MessageId {
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to MessageId");
        }
        let mut id = Self([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct ProgramId(pub [u8; 32]);

impl From<u64> for ProgramId {
    fn from(v: u64) -> Self {
        let mut id = ProgramId([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
    }
}

impl ProgramId {
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to ProgramId");
        }
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
