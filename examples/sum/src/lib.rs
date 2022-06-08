#![no_std]

use core::num::ParseIntError;
use gstd::{debug, exec, msg, prelude::*, ActorId};

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
pub unsafe extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    MESSAGE_LOG.push(format!("(sum) New msg: {:?}", new_msg));
    debug!("sum gas_available: {}", exec::gas_available());

    msg::send(STATE.send_to(), new_msg + new_msg, 0).unwrap();

    debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

    for log in MESSAGE_LOG.iter() {
        debug!(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let send_to = ActorId::from_slice(
        &decode_hex(&input).expect("INITIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
    STATE.set_send_to(send_to);
}
