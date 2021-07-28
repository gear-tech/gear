use crate::prelude::Vec;
use crate::structs::{MessageId, ProgramId};
use crate::sys;

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

pub fn id() -> MessageId {
    let mut msg_id = MessageId::default();
    unsafe { sys::gr_msg_id(msg_id.0.as_mut_ptr()) }
    msg_id
}

pub fn value() -> u128 {
    let mut value_data = [0u8; 16];
    unsafe {
        sys::gr_value(value_data.as_mut_ptr());
    }
    u128::from_le_bytes(value_data)
}

pub fn reply(payload: &[u8], gas_limit: u64, value: u128) {
    unsafe {
        sys::gr_reply(
            payload.as_ptr(),
            payload.len() as _,
            gas_limit,
            value.to_le_bytes().as_ptr(),
        )
    }
}

/// Transfers gas from program caller.
pub fn charge(gas: u64) {
    unsafe {
        sys::gr_charge(gas);
    }
}

pub fn reply_to() -> MessageId {
    let mut message_id = MessageId::default();
    unsafe { sys::gr_reply_to(message_id.0.as_mut_ptr()) }
    message_id
}

pub fn init(program: ProgramId, payload: &[u8], gas_limit: u64, value: u128) -> usize {
    unsafe {
        sys::gr_init(
            program.as_slice().as_ptr(),
            payload.as_ptr(),
            payload.len() as _,
            gas_limit,
            value.to_le_bytes().as_ptr(),
        ) as usize
    }
}

pub fn push(handle: usize, payload: &[u8]) {
    unsafe { sys::gr_push(handle as u32, payload.as_ptr(), payload.len() as _) }
}

pub fn commit(handle: usize) {
    unsafe { sys::gr_commit(handle as u32) }
}

pub fn push_reply(payload: &[u8]) {
    unsafe { sys::gr_push_reply(payload.as_ptr(), payload.len() as _) }
}
