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
