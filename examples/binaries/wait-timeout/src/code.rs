use crate::Command;

use codec::Encode;
use futures::future;
use gstd::{errors::ContractError, exec, msg, MessageId};

static mut TIMEOUT_MESSAGE_ID: Option<MessageId> = None;

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().expect("Failed to decode input");

    match cmd {
        Command::Wake => unsafe {
            if let Some(id) = TIMEOUT_MESSAGE_ID {
                exec::wake(id).expect("Failed to wake the message");
            }
        },
        Command::SendTimeout(to, duration) => {
            unsafe { TIMEOUT_MESSAGE_ID = Some(msg::id()) };

            let reply = msg::send_bytes_for_reply(to, b"", 0)
                .expect("send message failed")
                .up_to(duration)
                .await;

            if let Err(ContractError::Timeout(..)) = reply {
                let _ = msg::send(to, b"timeout", 0).expect("send message failed");
            }
        }
        Command::JoinTimeout(to, duration_a, duration_b) => {
            let reply = {
                let (a, b) = future::join(
                    msg::send_bytes_for_reply(to, b"", 0)
                        .expect("send message failed")
                        .up_to(duration_a),
                    msg::send_bytes_for_reply(to, b"", 0)
                        .expect("send message failed")
                        .up_to(duration_b),
                )
                .await;

                a.and_then(|ra| b.and_then(|rb| Ok((ra, rb))))
            };

            if let Err(ContractError::Timeout(..)) = reply {
                let _ = msg::send(to, b"timeout", 0).expect("send message failed");
            }
        }
        Command::SelectTimeout(to, duration_a, duration_b) => {
            let reply = match future::select(
                msg::send_bytes_for_reply(to, b"", 0)
                    .expect("send message failed")
                    .up_to(duration_a),
                msg::send_bytes_for_reply(to, b"", 0)
                    .expect("send message failed")
                    .up_to(duration_b),
            )
            .await
            {
                future::Either::Left((r, _)) | future::Either::Right((r, _)) => r,
            };

            if let Err(ContractError::Timeout(..)) = reply {
                let _ = msg::send(to, b"timeout", 0).expect("send message failed");
            }
        }
    }
}
