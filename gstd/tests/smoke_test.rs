#![no_std]

use gstd::prelude::*;
use gstd::{msg, ProgramId};

#[cfg(feature = "debug")]
use gstd::ext;

static mut PROGRAM: ProgramId = ProgramId([0; 32]);
static mut MESSAGE: Vec<u8> = Vec::new();
static mut GAS_LIMIT: u64 = 0;
static mut VALUE: u128 = 0;
static mut GAS: u64 = 0;

#[cfg(feature = "debug")]
static mut DEBUG_MSG: Vec<u8> = Vec::new();

mod sys {
    use super::*;

    #[no_mangle]
    unsafe extern "C" fn gr_send(
        program: *const u8,
        data_ptr: *const u8,
        data_len: u32,
        gas_limit: u64,
        value_ptr: *const u8,
    ) {
        ptr::copy(program, PROGRAM.0.as_mut_ptr(), 32);
        MESSAGE.resize(data_len as _, 0);
        ptr::copy(data_ptr, MESSAGE.as_mut_ptr(), data_len as _);
        GAS_LIMIT = gas_limit;
        VALUE = *(value_ptr as *const u128);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_size() -> u32 {
        MESSAGE.len() as u32
    }

    #[no_mangle]
    unsafe extern "C" fn gr_read(at: u32, len: u32, dest: *mut u8) {
        let src = MESSAGE.as_ptr();
        ptr::copy(src.offset(at as _), dest, len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_source(program: *mut u8) {
        for i in 0..PROGRAM.0.len() {
            *program.offset(i as isize) = PROGRAM.0[i];
        }
    }

    #[no_mangle]
    unsafe extern "C" fn gr_value(val: *mut u8) {
        let src = VALUE.to_ne_bytes().as_ptr();
        ptr::copy(src, val, mem::size_of::<u128>());
    }

    #[no_mangle]
    unsafe extern "C" fn gr_charge(gas: u64) {
        GAS += gas;
    }

    #[cfg(feature = "debug")]
    #[no_mangle]
    unsafe extern "C" fn gr_debug(msg_ptr: *const u8, msg_len: u32) {
        DEBUG_MSG.resize(msg_len as _, 0);
        ptr::copy(msg_ptr, DEBUG_MSG.as_mut_ptr(), msg_len as _);
    }
}

#[test]
fn messages() {
    let mut id: [u8; 32] = [0; 32];
    for i in 0..id.len() {
        id[i] = i as u8;
    }

    msg::send_with_value(ProgramId(id), b"HELLO", 1000, 12345678);

    let msg_source = msg::source();
    assert_eq!(msg_source, ProgramId(id));

    let msg_load = msg::load();
    assert_eq!(msg_load, b"HELLO");
}

#[test]
fn transfer_gas() {
    msg::charge(1000);
    unsafe {
        assert_eq!(GAS, 1000);
    }
    msg::charge(2000);
    unsafe {
        assert_eq!(GAS, 3000);
    }
}

#[cfg(feature = "debug")]
#[test]
fn debug() {
    ext::debug("DBG: test message");

    unsafe {
        assert_eq!(DEBUG_MSG, "DBG: test message".as_bytes());
    }
}
