#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

#[macro_use]
pub mod macros;
pub mod msg;
pub mod prelude;

pub use msg::{MessageId, ProgramId};

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
