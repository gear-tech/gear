#![no_std]

use codec::{Decode, Encode};
use gstd::{ext, msg, String, prelude::*};

#[derive(Debug, codec::Encode, codec::Decode)]
pub enum Action {
    AddMessage(Message),
    ViewMessages,
}

#[derive(Debug, codec::Encode, codec::Decode)]
pub struct Message {
    autor: String,
    msg: String,
}

static mut MESSAGES: Vec<Message> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let action: Action = msg::load().unwrap();

    match action {
        Action::AddMessage(message) => {
            MESSAGES.push(message);
            msg::reply(b"Message added!", 0, 0);
        },
        Action::ViewMessages => {
            msg::reply(&MESSAGES, 0, 0);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}