use crate::Kind;
use gstd::{
    exec, msg::{self, MessageHandle},
    prelude::{vec, *},
};

#[no_mangle]
extern "C" fn init() {}

#[gstd::async_main]
async fn main() {
    let kind: Kind = msg::load().expect("invalid arguments");
    let encoded_kind = kind.encode();

    match kind {
        Kind::Send => {
            msg::send_for_reply(msg::source(), kind, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendWithGas(gas) => {
            msg::send_with_gas_for_reply(msg::source(), kind, gas, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytes => {
            msg::send_bytes_for_reply(msg::source(), &encoded_kind, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytesWithGas(gas) => {
            msg::send_bytes_with_gas_for_reply(msg::source(), &encoded_kind, gas, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendCommit => {
            let handle = MessageHandle::init().expect("init message failed");
            handle.push(&encoded_kind).expect("push payload failed");
            handle.commit_for_reply(msg::source(), 0)
                .expect("send message failed")
                .await
        }
        Kind::SendCommitWithGas(gas) => {
            let handle = MessageHandle::init().expect("init message failed");
            handle.push(&encoded_kind).expect("push payload failed");
            handle.commit_with_gas_for_reply(msg::source(), gas, 0)
                .expect("send message failed")
                .await
        }
    }
    .expect("ran into error-reply");

    msg::send(msg::source(), b"PONG", 0).expect("send message failed");
}
