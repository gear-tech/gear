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
    match kind.clone() {
        Kind::Send => {
            msg::send_and_wait_for_reply::<Vec<u8>, Kind>(msg::source(), kind, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendWithGas(gas) => {
            msg::send_with_gas_and_wait_for_reply::<Vec<u8>, Kind>(msg::source(), kind, gas, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytes => {
            msg::send_bytes_and_wait_for_reply(msg::source(), kind.encode().to_vec(), 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytesWithGas(gas) => {
            msg::send_bytes_with_gas_and_wait_for_reply(
                msg::source(),
                kind.encode().to_vec(),
                gas,
                0,
            )
            .expect("send message failed")
            .await
        }
    }
    .expect("ran into error-reply");

    msg::reply(b"PONG", 0);
}
