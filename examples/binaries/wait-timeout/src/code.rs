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
        Command::WaitLost(to) => {
            let wait = msg::send_bytes_for_reply(to, b"ping", 0)
                .expect("send message failed")
                .up_to(Some(5))
                .expect("Invalid wait duration.");

            if let Err(e) = msg::send_bytes_for_reply(to, b"", 0)
                .expect("send message failed")
                .up_to(Some(10))
                .expect("Invalid wait duration.")
                .await
            {
                if e.timed_out() {
                    let _ = msg::send(to, b"timeout", 0).expect("send message failed");
                } else {
                    panic!("timeout has not been triggered.")
                }
            }

            if let Err(e) = wait.await {
                if e.timed_out() {
                    msg::send(to, b"timeout2", 0).expect("send message failed");
                } else {
                    panic!("timeout has not been triggered.")
                }
            }

            msg::send(to, b"success", 0).expect("send message failed");
        }
        Command::SendTimeout(to, duration) => {
            unsafe { TIMEOUT_MESSAGE_ID = Some(msg::id()) };

            let reply = msg::send_bytes_for_reply(to, b"", 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;

            if let Err(e) = reply {
                if e.timed_out() {
                    let _ = msg::send(to, b"timeout", 0).expect("send message failed");
                } else {
                    panic!("timeout has not been triggered.")
                }
            }
        }
        Command::JoinTimeout(to, duration_a, duration_b) => {
            // Join two waited messages, futures complete at
            // the same time when both of them are finished.
            let reply = {
                let (a, b) = future::join(
                    msg::send_bytes_for_reply(to, b"", 0)
                        .expect("send message failed")
                        .up_to(Some(duration_a))
                        .expect("Invalid wait duration."),
                    msg::send_bytes_for_reply(to, b"", 0)
                        .expect("send message failed")
                        .up_to(Some(duration_b))
                        .expect("Invalid wait duration."),
                )
                .await;

                a.and_then(|ra| b.and_then(|rb| Ok((ra, rb))))
            };

            if let Err(e) = reply {
                if e.timed_out() {
                    let _ = msg::send(to, b"timeout", 0).expect("send message failed");
                } else {
                    panic!("timeout has not been triggered.")
                }
            }
        }
        Command::SelectTimeout(to, duration_a, duration_b) => {
            // Select from two waited messages, futures complete at
            // the same time when one of them getting failed.
            let reply = match future::select(
                msg::send_bytes_for_reply(to, b"", 0)
                    .expect("send message failed")
                    .up_to(Some(duration_a))
                    .expect("Invalid wait duration."),
                msg::send_bytes_for_reply(to, b"", 0)
                    .expect("send message failed")
                    .up_to(Some(duration_b))
                    .expect("Invalid wait duration."),
            )
            .await
            {
                future::Either::Left((r, _)) | future::Either::Right((r, _)) => r,
            };

            if let Err(e) = reply {
                if e.timed_out() {
                    let _ = msg::send(to, b"timeout", 0).expect("send message failed");
                } else {
                    panic!("timeout has not been triggered.")
                }
            }
        }
    }
}
