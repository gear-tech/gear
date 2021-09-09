#![no_std]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*};

#[derive(Debug, Encode, Decode)]
pub enum Action {
    AddMessage(MessageIn),
    ViewMessages,
}

#[derive(Debug, Encode, Decode)]
pub struct MessageIn {
    author: Vec<u8>,
    msg: Vec<u8>,
}

static mut MESSAGES: Vec<MessageIn> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let action: Action = msg::load().unwrap();

    match action {
        Action::AddMessage(message) => {
            MESSAGES.push(message);
        }
        Action::ViewMessages => {
            msg::reply(&MESSAGES, 0, 0);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
