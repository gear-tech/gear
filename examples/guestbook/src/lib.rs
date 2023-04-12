#![no_std]
#![allow(deprecated)]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*};
use scale_info::TypeInfo;

#[derive(TypeInfo, Decode)]
pub enum Action {
    AddMessage(MessageIn),
    ViewMessages,
}

#[derive(TypeInfo, Decode, Encode)]
pub struct MessageIn {
    author: String,
    msg: String,
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "Guestbook",
    handle:
        input: Action,
        output: Vec<MessageIn>,
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
