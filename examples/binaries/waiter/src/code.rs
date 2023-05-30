use crate::Command;

use gstd::{errors::ContractError, exec, format, msg, MessageId};

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
                .await;
        }
        Command::SendUpTo(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;
        }
        Command::SendUpToWait(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
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
        Command::DelayFor(duration) => {
            msg::send(
                msg::source(),
                format!("Before the delay at block: {}", exec::block_height()),
                0,
            )
            .expect("Failed to send before the delay");
            exec::delay_for(duration).await;
            msg::send(
                msg::source(),
                format!("After the delay at block: {}", exec::block_height()),
                0,
            )
            .expect("Failed to reply after the delay");
        }
        Command::WakeUp(msg_id) => {
            exec::wake(msg_id.into()).expect("Failed to wake up the message");
        }
    }
}
