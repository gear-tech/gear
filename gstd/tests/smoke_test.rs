#![no_std]

use gstd::msg;
use gstd::prelude::*;
use gstd::*;
use gstd::{Gas, ProgramId};

#[cfg(feature = "debug")]
use gstd::ext;

static mut PROGRAM: ProgramId = ProgramId([0; 32]);
static mut MESSAGE: Vec<u8> = Vec::new();
static mut GAS_LIMIT: u64 = 0;
static mut VALUE: u128 = 0;
static mut GAS: Gas = Gas(0);

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
        GAS += Gas(gas);
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

    msg::send_with_value(ProgramId(id), b"HELLO", Gas(1000), 12345678);

    let msg_source = msg::source();
    assert_eq!(msg_source, ProgramId(id));

    let msg_load = msg::load();
    assert_eq!(msg_load, b"HELLO");
}

#[test]
fn transfer_gas() {
    msg::charge(Gas(1000));
    unsafe {
        assert_eq!(GAS, Gas(1000));
    }
    msg::charge(Gas(2000));
    unsafe {
        assert_eq!(GAS, Gas(3000));
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

#[test]
fn gas_macro() {
    assert_eq!(gas!(), Gas(0));
    assert_eq!(gas!(1234), Gas(1234));

    assert_eq!(gas!(1 K), Gas(1_000));
    assert_eq!(gas!(1 M), Gas(1_000_000));
    assert_eq!(gas!(1 G), Gas(1_000_000_000));
    assert_eq!(gas!(1 T), Gas(1_000_000_000_000));

    assert_eq!(gas!(2.7 K), Gas(2_700));
    assert_eq!(gas!(0.6 M), Gas(600_000));
    assert_eq!(gas!(1002 K), Gas(1_002_000));
}

struct SomeType(usize);

#[derive(Debug)]
struct SomeError;

#[test]
fn bail_ok() {
    let res: Result<SomeType, SomeError> = Ok(SomeType(0));
    let val = bail!(res, "Your static explanation for both features");
    assert_eq!(val.0, 0);

    let res: Result<SomeType, SomeError> = Ok(SomeType(1));
    let val = bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
    assert_eq!(val.0, 1);

    let res: Result<SomeType, SomeError> = Ok(SomeType(2));
    let val = bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
    assert_eq!(val.0, 2);

    let res: Result<SomeType, SomeError> = Ok(SomeType(3));
    let val = bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
    assert_eq!(val.0, 3);
}

#[test]
#[should_panic(expected = "Your static explanation for both features")]
fn bail_err_general_message() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(res, "Your static explanation for both features");
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_no_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "Your static debug explanation")]
fn bail_err_no_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_single_arg_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "It was formatted -> 0: SomeError")]
fn bail_err_single_arg_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_multiple_args_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "They were formatted -> 0 SECOND_ARG: SomeError")]
fn bail_err_multiple_args_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
}
