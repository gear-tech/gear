use crate::Command;

use codec::Encode;
use gstd::{errors::ContractError, exec, msg, MessageId};

static mut TIMEOUT_MESSAGE_ID: Option<MessageId> = None;

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wake => unsafe {
            if let Some(id) = TIMEOUT_MESSAGE_ID {
                exec::wake(id);
            }
        },
        Command::WaitMore(duration) => {
            let _ = msg::reply_bytes_for_reply([], 0)
                .expect("send reply failed")
                .exactly(duration)
                .await;
        }
        Command::SendTimeout(to, duration) => {
            unsafe { TIMEOUT_MESSAGE_ID = Some(msg::id()) };

            let reply = msg::send_bytes_for_reply(
                exec::program_id(),
                Command::WaitMore(duration).encode(),
                0,
            )
            .expect("send message failed")
            .exactly(duration)
            .await;

            if let Err(ContractError::Timeout(..)) = reply {
                let _ = msg::send(to, b"timeout", 0).expect("send message failed");
            }
        }
    }
}
