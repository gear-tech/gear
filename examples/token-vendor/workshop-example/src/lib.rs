#![no_std]

use gstd::{debug, msg, prelude::*, ActorId};

gstd::metadata! {
    title: "Gear Workshop Contract Example",
    init:
        output: String,
    handle:
        input: String,
        output: String,
}

struct State {
    user_id: Option<ActorId>,
}

impl State {
    fn set_user_id(&mut self, user_id: ActorId) {
        self.user_id = Some(user_id);
    }

    fn get_hex_id(&self) -> String {
        let id = self.user_id.unwrap_or_default();

        hex::encode(id.as_ref())
    }
}

static mut STATE: State = State { user_id: None };

#[no_mangle]
pub unsafe extern "C" fn init() {
    STATE.set_user_id(msg::source());

    debug!("CONTRACT: Inited successfully");
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let payload: String = msg::load().expect("CONTRACT: Unable to decode handle input");

    debug!("CONTRACT: Got payload: '{}'", payload);

    if payload == "success" {
        msg::reply(STATE.get_hex_id(), 0).unwrap();
    } else if payload == "ping" {
        msg::reply("pong", 0).unwrap();
    } else {
        // Do nothing
    }
}
