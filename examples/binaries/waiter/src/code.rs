use crate::{Command, WaitSubcommand};

use gstd::{errors::ContractError, exec, format, msg, MessageId};

fn process_wait_subcommand(subcommand: WaitSubcommand) {
    match subcommand {
        WaitSubcommand::Wait => exec::wait(),
        WaitSubcommand::WaitFor(duration) => exec::wait_for(duration),
        WaitSubcommand::WaitUpTo(duration) => exec::wait_up_to(duration),
    }
}

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait(subcommand) => process_wait_subcommand(subcommand),
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
        Command::ReplyAndWait(subcommand) => {
            msg::reply("", 0).expect("Failed to send reply");

            process_wait_subcommand(subcommand);
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
