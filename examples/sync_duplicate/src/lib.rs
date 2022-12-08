#![no_std]

use gstd::{msg, prelude::*, ActorId};

static mut DEST: ActorId = ActorId::new([0u8; 32]);
static mut COUNTER: usize = 0;

#[no_mangle]
extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    unsafe {
        DEST = ActorId::from_slice(&hex::decode(dest).expect("Invalid hex"))
            .expect("Unable to create ActorId")
    };
}

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    if &msg == "async" {
        unsafe { COUNTER += 1 };

        let _ = msg::send_bytes_for_reply(unsafe { DEST }, "PING", 0)
            .unwrap()
            .await
            .expect("Error in async message processing");

        msg::reply(unsafe { COUNTER } as i32, 0).unwrap();

        unsafe { COUNTER = 0 };
    };
}
