#[cfg(feature = "debug")]
pub mod ext {
    pub fn debug(s: &str) {
        unsafe { super::gr_debug(s.as_ptr(), s.as_bytes().len() as _) }
    }
}

extern "C" {
    pub fn gr_charge(gas: u64);
    pub fn gr_commit(handle: u32);
    #[cfg(feature = "debug")]
    pub fn gr_debug(msg_ptr: *const u8, msg_len: u32);
    pub fn gr_init(
        program: *const u8,
        data_ptr: *const u8,
        data_len: u32,
        gas_limit: u64,
        value_ptr: *const u8,
    ) -> u32;
    pub fn gr_msg_id(val: *mut u8);
    pub fn gr_push(handle: u32, data_ptr: *const u8, data_len: u32);
    pub fn gr_push_reply(data_ptr: *const u8, data_len: u32);
    pub fn gr_read(at: u32, len: u32, dest: *mut u8);
    pub fn gr_reply(data_ptr: *const u8, data_len: u32, gas_limit: u64, value_ptr: *const u8);
    pub fn gr_reply_to(dest: *mut u8);
    pub fn gr_send(
        program: *const u8,
        data_ptr: *const u8,
        data_len: u32,
        gas_limit: u64,
        value_ptr: *const u8,
    );
    pub fn gr_size() -> u32;
    pub fn gr_source(program: *mut u8);
    pub fn gr_value(val: *mut u8);
}
