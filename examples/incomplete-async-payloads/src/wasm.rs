use crate::Command;
use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
    ActorId,
};

static mut DESTINATION: ActorId = ActorId::zero();

#[no_mangle]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

async fn ping() {
    msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
}

#[gstd::async_main]
async fn main() {
    let command = msg::load().expect("Failed to load command");

    match command {
        Command::HandleStore => {
            let handle = MessageHandle::init().expect("Failed to init message");
            handle.push(b"STORED ").expect("Failed to push payload");
            ping().await;
            handle.push("COMMON").expect("Failed to push payload");
            handle
                .commit(msg::source(), 0)
                .expect("Failed to commit message");
        }
        Command::ReplyStore => {
            msg::reply_push(b"STORED ").expect("Failed to push reply payload");
            ping().await;
            msg::reply_push(b"REPLY").expect("Failed to push reply payload");
            msg::reply_commit(0).expect("Failed to commit reply");
        }
        Command::Handle => {
            let handle = MessageHandle::init().expect("Failed to init message");
            handle.push(b"OK PING").expect("Failed to push payload");
            handle
                .commit(msg::source(), 0)
                .expect("Failed to commit message");
        }
        Command::Reply => {
            msg::reply_push(b"OK REPLY").expect("Failed to push reply payload");
            msg::reply_commit(0).expect("Failed to commit reply");
        }
        Command::ReplyTwice => {
            msg::reply_bytes("FIRST", 0).expect("Failed to send reply");
            ping().await;
            // Won't be sent, because one
            // execution allows only one reply
            msg::reply_bytes("SECOND", 0).expect("Failed to send reply");
        }
    }
}
