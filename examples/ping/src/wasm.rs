use gstd::{msg, prelude::*};

#[no_mangle]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"PING" {
        msg::reply_bytes("PONG", 0).expect("Failed to send reply");
    }
}
