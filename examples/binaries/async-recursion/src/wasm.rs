use async_recursion::async_recursion;
use gstd::{msg, prelude::*, ActorId};

static mut DESTINATION: ActorId = ActorId::zero();

#[no_mangle]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

/// Send message "PING" and wait for a reply, then recursively
/// repeat with `val` decreased by reply len while `val` > reply len.
#[async_recursion]
async fn rec_func(val: i32) {
    let reply = msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");

    msg::send(msg::source(), val, 0).expect("Failed to send message");

    let reply_len = reply.len() as i32;

    if val - reply_len > 0 {
        rec_func(val - reply_len).await;
    }
}

#[gstd::async_main]
async fn main() {
    let arg = msg::load().expect("Failed to load argument");
    rec_func(arg).await;
}
