use crate::{Hash, LengthWithCode};

pub const GET_STACK_BUFFER_GLOBAL_NAME: &str = "get_stack_buffer_global";
pub const SET_STACK_BUFFER_GLOBAL_NAME: &str = "set_stack_buffer_global";

extern "C" {
    pub fn get_stack_buffer_global() -> u64;
    pub fn set_stack_buffer_global(i: u64);
}

#[repr(C, align(0x4000))]
pub struct StackBuffer {
    pub block_height: u32,
    pub block_timestamp: u64,
    pub message_id: Hash,
    pub origin: Hash,
    pub message_size: usize,
    pub status_code: LengthWithCode,
    pub source: Hash,
    pub value: u128,
}

pub const STACK_BUFFER_SIZE: u32 = core::mem::size_of::<StackBuffer>() as u32;
