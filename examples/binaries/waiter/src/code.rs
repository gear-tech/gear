use crate::Command;

use gstd::{errors::ContractError, exec, msg};

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait => exec::wait(),
        Command::WaitFor(duration) => exec::wait_for(duration),
        Command::WaitNoMore(duration) => exec::wait_no_more(duration),
        Command::SendFor(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .exactly(duration)
                .await;
        }
        Command::SendNoMore(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .no_more(duration)
                .await;
        }
        Command::SendNoMoreWait(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .no_more(duration)
                .await;

            // after waking, wait again.
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .await;
        }
        Command::SendTimeout(to, duration) => {
            let _ = match msg::send_bytes_for_reply(Default::default(), [], 0)
                .expect("send message failed")
                .no_more(duration)
                .await
            {
                Err(e) => match e {
                    ContractError::Timeout(..) => msg::send(to, b"timeout", 0),
                    _ => msg::send(to, b"unknown error", 0),
                },
                _ => unreachable!("This could not happen"),
            }
            .expect("send message failed");
        }
    }
}
