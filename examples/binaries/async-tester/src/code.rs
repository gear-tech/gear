use crate::Kind;
use gstd::{
    exec, msg,
    prelude::{vec, *},
};

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[gstd::async_main]
async fn main() {
    let kind: Kind = msg::load().expect("invalid arguments");
    let encoded_kind = kind.encode().to_vec();

    match kind {
        Kind::ReplyWithGas(gas) => msg::reply_with_gas_for_reply(&encoded_kind, gas, 0).await,
        Kind::ReplyBytes => msg::reply_bytes_for_reply(&encoded_kind, 0).await,
        Kind::ReplyBytesWithGas(gas) => {
            msg::reply_bytes_with_gas_for_reply(&encoded_kind, gas, 0).await
        }
        Kind::ReplyCommit => {
            msg::reply_push(&encoded_kind).expect("push payload failed");
            msg::reply_commit_for_reply(0).await
        }
        Kind::ReplyCommitWithGas(gas) => {
            msg::reply_push(&encoded_kind).expect("push payload failed");
            msg::reply_commit_with_gas_for_reply(gas, 0).await
        }
        Kind::SendBytes => msg::send_bytes_for_reply(msg::source(), &encoded_kind, 0).await,
        Kind::SendBytesWithGas(gas) => {
            msg::send_bytes_with_gas_for_reply(msg::source(), &encoded_kind, gas, 0).await
        }
        Kind::SendCommit => {
            let handle = msg::send_init().expect("init message failed");
            msg::send_push(&handle, &encoded_kind).expect("push payload failed");
            msg::send_commit_for_reply(handle, msg::source(), 0).await
        }
        Kind::SendCommitWithGas(gas) => {
            let handle = msg::send_init().expect("init message failed");
            msg::send_push(&handle, &encoded_kind).expect("push payload failed");
            msg::send_commit_with_gas_for_reply(handle, msg::source(), gas, 0).await
        }
    }
    .expect("ran into error-reply");

    match kind {
        Kind::ReplyWithGas(_)
        | Kind::ReplyBytes
        | Kind::ReplyBytesWithGas(_)
        | Kind::ReplyCommit
        | Kind::ReplyCommitWithGas(_) => {
            msg::send(msg::source(), b"PONG", 0).expect("send message failed")
        }
        _ => msg::reply(b"PONG", 0).expect("reply failed"),
    };
}
