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
                .exactly(Some(duration))
                .expect("Invalid wait duration.")
                .await
                .expect("Received error reply");
        }
        Command::SendUpTo(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await
                .expect("Received error reply");
        }
        Command::SendUpToWait(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await
                .expect("Received error reply");

            // after waking, wait again.
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .await
                .expect("Received error reply");
        }
        Command::SendAndWaitFor(duration, to) => {
            msg::send(to, b"ping", 0).expect("Failed to send message");
            exec::wait_for(duration);
        }
    }
}
