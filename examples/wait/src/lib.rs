#![no_std]

use gcore::{exec, msg, MessageId};

// for panic/oom handlers
extern crate gstd;

static mut STATE: u32 = 0;
static mut MSG_ID_1: MessageId = MessageId([0; 32]);
static mut MSG_ID_2: MessageId = MessageId([0; 32]);

#[no_mangle]
unsafe extern "C" fn handle() {
    gstd::debug!(STATE);
    match STATE {
        0 => {
            STATE = 1;
            MSG_ID_1 = msg::id();
            exec::wait();
        }
        1 => {
            STATE = 2;
            MSG_ID_2 = msg::id();
            exec::wait();
        }
        2 => {
            STATE = 3;
            exec::wake(MSG_ID_1).unwrap();
            exec::wake(MSG_ID_2).unwrap();
        }
        _ => {
            msg::send(msg::source(), b"WAITED", 0).unwrap();
        }
    }
}
