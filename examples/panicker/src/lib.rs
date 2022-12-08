#![no_std]

use gstd::debug;

#[no_mangle]
extern "C" fn handle() {
    debug!("Starting panicker handle");
    panic!("I just panic every time")
}
