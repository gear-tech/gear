use gstd::{msg, prelude::*};
use parity_scale_codec::{Decode, Encode};

#[derive(Decode)]
pub enum Action {
    AddMessage(MessageIn),
    ViewMessages,
}

#[derive(Decode, Encode)]
pub struct MessageIn {
    author: String,
    msg: String,
}

static mut MESSAGES: Vec<MessageIn> = Vec::new();

#[no_mangle]
extern "C" fn handle() {
    let action: Action = msg::load().unwrap();

    match action {
        Action::AddMessage(message) => {
            unsafe { MESSAGES.push(message) };
        }
        Action::ViewMessages => {
            msg::reply(unsafe { &MESSAGES }, 0).unwrap();
        }
    }
}
