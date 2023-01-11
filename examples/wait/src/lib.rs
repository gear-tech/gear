#![no_std]

use gcore::{exec, msg, MessageId};

// for panic/oom handlers
extern crate gstd;

static mut STATE: u32 = 0;
static mut MSG_ID_1: MessageId = MessageId([0; 32]);
static mut MSG_ID_2: MessageId = MessageId([0; 32]);

#[no_mangle]
extern "C" fn handle() {
    let state = unsafe { &mut STATE };
    gstd::debug!(state);
    match *state {
        0 => {
            *state = 1;
            unsafe { MSG_ID_1 = msg::id() };
            exec::wait();
        }
        1 => {
            *state = 2;
            unsafe { MSG_ID_2 = msg::id() };
            exec::wait();
        }
        2 => {
            *state = 3;
            exec::wake(unsafe { MSG_ID_1 }).unwrap();
            exec::wake(unsafe { MSG_ID_2 }).unwrap();
        }
        _ => {
            msg::send(msg::source(), b"WAITED", 0).unwrap();
        }
    }
}
