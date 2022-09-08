use crate::Command;

use gstd::{exec, msg, traits::Wait};

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
                .till(duration)
                .await;
        }
        Command::SendNoMore(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .no_more(duration)
                .await;
        }
    }
}
