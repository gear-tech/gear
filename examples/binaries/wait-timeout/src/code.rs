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
        Command::Wait(to) => {
            msg::send_bytes_for_reply(to, b"", 0)
                .expect("send reply failed")
                .await;

            msg::reply(b"", 0).expect("failed to send reply");
        }
        Command::SendTimeout(to, duration) => {
            unsafe { TIMEOUT_MESSAGE_ID = Some(msg::id()) };

            let reply =
                msg::send_bytes_for_reply(exec::program_id(), Command::Wait(to).encode(), 0)
                    .expect("send message failed")
                    .no_more(duration)
                    .await;

            if let Err(ContractError::Timeout(..)) = reply {
                let _ = msg::send(to, b"timeout", 0).expect("send message failed");
            }
        }
    }
}
