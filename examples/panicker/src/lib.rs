#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::prelude::*;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    panic!("I just panic every time")
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
