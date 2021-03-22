#![no_std]

extern crate alloc;

#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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


pub mod msg {
    use alloc::vec::Vec;
    use super::ProgramId;

    mod sys {
        extern "C" {
            pub fn send(program: *const u8, data_ptr: *const u8, data_len: u32);
            pub fn size() -> u32;
            pub fn read(at: u32, len: u32, dest: *mut u8);
            pub fn source(program: *mut u8);
        }
    }

    pub fn load() -> Vec<u8> {
        unsafe {
            let message_size = sys::size() as usize;
            let mut data = Vec::with_capacity(message_size);
            data.set_len(message_size);
            sys::read(0, message_size as _, data.as_mut_ptr() as _);
            data
        }
    }

    pub fn send(program: ProgramId, payload: &[u8]) {
        unsafe {
            sys::send(program.as_slice().as_ptr(), payload.as_ptr(), payload.len() as _)
        }
    }

    pub fn source() -> ProgramId {
        let mut program_id = ProgramId::default();
        unsafe { sys::source(program_id.as_mut_slice().as_mut_ptr()) }
        program_id
    }

}

#[cfg(feature = "debug")]
pub mod ext {
    mod sys {
        extern "C" {
            pub fn debug(msg_ptr: *const u8, msg_len: u32);
        }
    }

    pub fn debug(s: &str) {
        unsafe {
            sys::debug(s.as_ptr(), s.as_bytes().len() as _)
        }
    }
}
