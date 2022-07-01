#![no_std]

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

gstd::metadata! {
    title: "Guestbook",
    handle:
        input: Action,
        output: Vec<MessageIn>,
}

static mut MESSAGES: Vec<MessageIn> = Vec::new();

#[no_mangle]
unsafe extern "C" fn handle() {
    let action: Action = msg::load().unwrap();

    match action {
        Action::AddMessage(message) => {
            MESSAGES.push(message);
        }
        Action::ViewMessages => {
            msg::reply(&MESSAGES, 0).unwrap();
        }
    }
}
