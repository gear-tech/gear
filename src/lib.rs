#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

extern crate alloc;

#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

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

#[macro_export]
macro_rules! bail {
    ($some:expr, $expl:expr, $fmt:expr, $($arg:tt)*) => {
        match () {
            #[cfg(feature = "debug")]
            () => $some.expect(&format!($fmt, $($arg:tt)*)),
            #[cfg(not(feature = "debug"))]
            () => match $some {
                Ok(v) => v,
                Err(_) => core::panic!($expl),
            }
        }
    };
}

pub mod msg {
    use super::ProgramId;
    use alloc::vec::Vec;

    mod sys {
        extern "C" {
            pub fn gr_send(
                program: *const u8,
                data_ptr: *const u8,
                data_len: u32,
                gas_limit: u64,
                value_ptr: *const u8,
            );
            pub fn gr_size() -> u32;
            pub fn gr_read(at: u32, len: u32, dest: *mut u8);
            pub fn gr_source(program: *mut u8);
            pub fn gr_value(val: *mut u8);
        }
    }

    pub fn load() -> Vec<u8> {
        unsafe {
            let message_size = sys::gr_size() as usize;
            let mut data = Vec::with_capacity(message_size);
            data.set_len(message_size);
            sys::gr_read(0, message_size as _, data.as_mut_ptr() as _);
            data
        }
    }

    pub fn send(program: ProgramId, payload: &[u8], gas_limit: u64, value: u128) {
        unsafe {
            sys::gr_send(
                program.as_slice().as_ptr(),
                payload.as_ptr(),
                payload.len() as _,
                gas_limit,
                value.to_le_bytes().as_ptr(),
            )
        }
    }

    pub fn source() -> ProgramId {
        let mut program_id = ProgramId::default();
        unsafe { sys::gr_source(program_id.as_mut_slice().as_mut_ptr()) }
        program_id
    }

    pub fn value() -> u128 {
        let mut value_data = [0u8; 16];
        unsafe {
            sys::gr_value(value_data.as_mut_ptr());
        }
        u128::from_le_bytes(value_data)
    }
}

#[cfg(feature = "debug")]
pub mod ext {
    mod sys {
        extern "C" {
            pub fn gr_debug(msg_ptr: *const u8, msg_len: u32);
        }
    }

    pub fn debug(s: &str) {
        unsafe { sys::gr_debug(s.as_ptr(), s.as_bytes().len() as _) }
    }
}
