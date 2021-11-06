#![no_std]

use core::num::ParseIntError;
use gstd::{debug, exec, msg, prelude::*, ProgramId};

static mut MESSAGE_LOG: Vec<String> = vec![];

static mut STATE: State = State {
    send_to: ProgramId([0u8; 32]),
};
#[derive(Debug)]
struct State {
    send_to: ProgramId,
}

impl State {
    fn set_send_to(&mut self, to: ProgramId) {
        self.send_to = to;
    }
    fn send_to(&self) -> ProgramId {
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

    if exec::gas_available() > 4_000_000_000 {
        msg::send(STATE.send_to(), new_msg + new_msg, 4_000_000_000, 0);

        debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

        for log in MESSAGE_LOG.iter() {
            debug!(log);
        }
    } else {
        debug!("Not enough gas");
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let send_to = ProgramId::from_slice(
        &decode_hex(&input).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    );
    STATE.set_send_to(send_to);
}
