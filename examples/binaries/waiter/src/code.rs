use crate::Command;

use gstd::{exec, msg};

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait => exec::wait(),
        Command::WaitFor(duration) => exec::wait_for(duration),
        Command::WaitUpTo(duration) => exec::wait_up_to(duration),
        Command::SendFor(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .exactly(duration)
                .await;
        }
        Command::SendNoMore(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(duration)
                .await;
        }
        Command::SendNoMoreWait(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(duration)
                .await;

            // after waking, wait again.
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .await;
        }
        Command::SendAndWaitFor(duration, to) => {
            msg::send(to, b"ping", 0);
            exec::wait_for(duration);
        }
    }
}
