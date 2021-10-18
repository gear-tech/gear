#![no_std]

use gcore::{exec, msg, MessageId};

// for panic/oom handlers
extern crate gstd;

static mut STATE: u32 = 0;
static mut MSG_ID: MessageId = MessageId([0; 32]);

#[no_mangle]
pub unsafe extern "C" fn handle() {
    match STATE {
        0 => {
            STATE = 1;
            MSG_ID = msg::id();
            exec::wait();
        }
        1 => {
            STATE = 2;
            exec::wake(MSG_ID, exec::gas_available());
        }
        _ => {
            msg::send(msg::source(), b"WAITED", 1_000_000, 0);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
