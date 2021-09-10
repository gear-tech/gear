#![no_std]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*};
use gstd_meta::{meta, TypeInfo};

#[derive(TypeInfo, Decode)]
pub enum Action {
    AddMessage(MessageIn),
    ViewMessages,
}

#[derive(TypeInfo, Decode, Encode)]
pub struct MessageIn {
    author: Vec<u8>,
    msg: Vec<u8>,
}

meta! {
    title: "Guestbook",
    input: Action,
    output: Vec<MessageIn>,
    init_input: i32,
    init_output: i32,
    extra: MessageIn
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
