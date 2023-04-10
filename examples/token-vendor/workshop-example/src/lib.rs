#![no_std]
#![allow(deprecated)]

use gstd::{debug, msg, prelude::*, ActorId};

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
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
extern "C" fn init() {
    unsafe { STATE.set_user_id(msg::source()) };

    debug!("CONTRACT: Inited successfully");
}

#[no_mangle]
extern "C" fn handle() {
    let payload: String = msg::load().expect("CONTRACT: Unable to decode handle input");

    debug!("CONTRACT: Got payload: '{}'", payload);

    if payload == "success" {
        msg::reply(unsafe { STATE.get_hex_id() }, 0).unwrap();
    } else if payload == "ping" {
        msg::reply("pong", 0).unwrap();
    } else {
        // Do nothing
    }
}
