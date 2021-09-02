#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct MessageInitIn {
    count: u8,
}

#[derive(Serialize)]
struct MessageInitOut {
    old_count: u8,
    is_odd: bool,
    double_count: u8,

}

impl From<MessageInitIn> for MessageInitOut {
    fn from(msg: MessageInitIn) -> Self {
        Self {
            old_count: msg.count,
            is_odd: msg.count % 2 == 0,
            double_count: msg.count * 2,
        }
    }
}

#[derive(Deserialize)]
struct MessageIn {
    annotation: String,
    value: u32,
}

#[derive(Serialize)]
struct MessageOut {
    answer: String,
    incoming_value: u32,
    output: Vec<u32>,
}

impl From<MessageIn> for MessageOut {
    fn from(msg: MessageIn) -> Self {
        let answer: String = match msg.annotation == "ping" {
            true => "pong",
            _ => "not pong",
        }.into();

        let mut output = vec![];
        for i in 0..msg.value {
            output.push(2u32.pow(i));
        }

        Self {
            answer,
            incoming_value: msg.value,
            output,
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let msg_in: MessageIn = msg::load_custom().expect("Invalid MessageIn");

    let msg_out = MessageOut::from(msg_in);
    
    msg::send_custom(msg::source(), msg_out, 1_000_000_000);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let msg_init_in: MessageInitIn = msg::load_custom().expect("Invalid MessageInitIn");

    let msg_init_out: MessageInitOut = msg_init_in.into();
    
    msg::send_custom(0u64.into(), msg_init_out, 1_000_000_000);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}
