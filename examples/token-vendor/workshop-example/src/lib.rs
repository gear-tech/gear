#![no_std]

use gstd::{exec, ext, msg, prelude::*, ProgramId};

gstd::metadata! {
    title: "GEAR Workshop Contract Example",
    init:
        output: String,
    handle:
        input: String,
        output: String
}

struct State {
    user_id: Option<ProgramId>,
}

impl State {
    fn set_user_id(&mut self, user_id: ProgramId) {
        self.user_id = Some(user_id);
    }

    fn get_hex_id(&self) -> String {
        let id = self.user_id.unwrap_or_default();

        hex::encode(id.as_slice())
    }
}

static mut STATE: State = State { user_id: None };
const GAS_RESERVE: u64 = 10_000_000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    STATE.set_user_id(msg::source());

    ext::debug("CONTRACT: Inited successfully");
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let payload: String = msg::load().unwrap_or_else(|_| {
        ext::debug("CONTRACT: Unable to decode handle input");
        panic!()
    });

    ext::debug(&format!("CONTRACT: Got payload: '{}'", payload));

    if payload == "success" {
        msg::reply(STATE.get_hex_id(), exec::gas_available() - GAS_RESERVE, 0);
    } else if payload == "ping" {
        msg::reply("pong", exec::gas_available() - GAS_RESERVE, 0);
    } else {
        // Do nothing
    }
}
