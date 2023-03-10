#![no_std]

use core::num::ParseIntError;
use gstd::{debug, msg, prelude::*, ActorId};

static mut MESSAGE_LOG: Vec<String> = vec![];

static mut STATE: State = State {
    send_to: ActorId::new([0u8; 32]),
};
#[derive(Debug)]
struct State {
    send_to: ActorId,
}

impl State {
    fn set_send_to(&mut self, to: ActorId) {
        self.send_to = to;
    }
    fn send_to(&self) -> ActorId {
        self.send_to
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[no_mangle]
extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    unsafe { MESSAGE_LOG.push(format!("(sum) New msg: {new_msg:?}")) };

    msg::send(unsafe { STATE.send_to() }, new_msg + new_msg, 0).unwrap();

    debug!("{:?} total message(s) stored: ", unsafe {
        MESSAGE_LOG.len()
    });

    for log in unsafe { MESSAGE_LOG.iter() } {
        debug!(log);
    }
}

#[no_mangle]
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    let send_to = ActorId::from_slice(
        &decode_hex(&input).expect("INITIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
    unsafe { STATE.set_send_to(send_to) };
}
