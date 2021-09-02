#![no_std]

// for panic/oom handlers
extern crate gstd;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    panic!("I just panic every time")
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
