#![no_std]

extern crate alloc;

#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

pub mod msg {
    use alloc::vec::Vec;

    mod sys {
        extern "C" {
            pub fn send(program: i64, data_ptr: *const u8, data_len: u32);
            pub fn size() -> u32;
            pub fn read(at: u32, len: u32, dest: *mut u8);
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

    pub fn send(program: u64, payload: &[u8]) {
        unsafe {
            sys::send(program as _, payload.as_ptr(), payload.len() as _)
        }
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
