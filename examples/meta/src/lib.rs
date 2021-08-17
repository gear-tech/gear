#![no_std]
#![feature(default_alloc_error_handler)]

use codec::{Decode, Encode};
use gstd::{ext, msg, prelude::*};

static mut CURRENT_VALUE: u64 = 0;

#[derive(Debug, Encode, Decode)]
struct MessageIn {
    value: u64,
    annotation: String,
}

#[derive(Debug, Encode, Decode)]
struct MessageOut {
    old_value: u64,
    new_value: u64,
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let message_in: MessageIn = msg::load().expect("Failed to decode incoming message");
    let old_value = CURRENT_VALUE;
    CURRENT_VALUE += message_in.value;
    ext::debug(&format!(
        "Increased with annotation: {}",
        message_in.annotation
    ));

    msg::reply(
        MessageOut {
            old_value,
            new_value: CURRENT_VALUE,
        },
        1000000,
        0,
    )
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let message_in: MessageIn = msg::load().expect("Failed to decode incoming message");
    CURRENT_VALUE = message_in.value;

    msg::reply(
        MessageOut {
            old_value: 0,
            new_value: CURRENT_VALUE,
        },
        1000000,
        0,
    )
}

fn return_slice<T>(slice: &[T]) -> *mut [i32; 2] {
    Box::into_raw(Box::new([
        slice.as_ptr() as isize as _,
        slice.len() as isize as _,
    ]))
}

#[no_mangle]
pub unsafe extern "C" fn meta_input() -> *mut [i32; 2] {
    return_slice(br#"{ "value": "u64", "annotation": "String" }"#)
}

#[no_mangle]
pub unsafe extern "C" fn meta_output() -> *mut [i32; 2] {
    return_slice(br#"{ "old_value": "u64", "new_value": "u64" }"#)
}

#[no_mangle]
pub unsafe extern "C" fn meta_title() -> *mut [i32; 2] {
    return_slice(br#"Example program with metadata"#)
}

#[no_mangle]
pub unsafe extern "C" fn meta_init_input() -> *mut [i32; 2] {
    return_slice(br#"{ "value": "u64", "annotation": "String" }"#)
}

#[no_mangle]
pub unsafe extern "C" fn meta_init_output() -> *mut [i32; 2] {
    return_slice(br#"{ "old_value": "u64", "new_value": "u64" }"#)
}
