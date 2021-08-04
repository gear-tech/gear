use crate::prelude::Vec;

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

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct MessageId(pub [u8; 32]);

impl MessageId {
    pub fn from_slice(s: &[u8]) -> Self {
        assert_eq!(s.len(), 32);
        let mut id = Self([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
}

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
        pub fn gr_reply_to(dest: *mut u8);
        pub fn gr_source(program: *mut u8);
        pub fn gr_value(val: *mut u8);
        pub fn gr_msg_id(val: *mut u8);
        pub fn gr_reply(data_ptr: *const u8, data_len: u32, gas_limit: u64, value_ptr: *const u8);
        pub fn gr_charge(gas: u64);
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

/// Transfer gas from program caller.
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
