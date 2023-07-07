use crate::{
    Command, MxLockContinuation, MxLockStaticAccessSubcommand, RwLockContinuation, RwLockType,
    SleepForWaitType, WaitSubcommand,
};
use core::ops::{Deref, DerefMut};
use futures::future;
use gstd::{errors::Error, exec, format, lock, msg, MessageId};

static mut MUTEX: lock::Mutex<()> = lock::Mutex::new(());
static mut MUTEX_LOCK_GUARD: Option<lock::MutexGuard<()>> = None;
static mut RW_LOCK: lock::RwLock<()> = lock::RwLock::new(());

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait(subcommand) => process_wait_subcommand(subcommand),
        Command::SendFor(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0, 0)
                .expect("send message failed")
                .exactly(Some(duration))
                .expect("Invalid wait duration.")
                .await;
        }
        Command::SendUpTo(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0, 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;
        }
        Command::SendUpToWait(to, duration) => {
            msg::send_bytes_for_reply(to, [], 0, 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;

            // after waking, wait again.
            msg::send_bytes_for_reply(to, [], 0, 0)
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
        Command::SleepFor(durations, wait_type) => {
            msg::send(
                msg::source(),
                format!("Before the sleep at block: {}", exec::block_height()),
                0,
            )
            .expect("Failed to send before the sleep");
            let sleep_futures = durations.iter().map(|duration| exec::sleep_for(*duration));
            match wait_type {
                SleepForWaitType::All => {
                    future::join_all(sleep_futures).await;
                }
                SleepForWaitType::Any => {
                    future::select_all(sleep_futures).await;
                }
                _ => unreachable!(),
            }
            msg::send(
                msg::source(),
                format!("After the sleep at block: {}", exec::block_height()),
                0,
            )
            .expect("Failed to send after the sleep");
        }
        Command::WakeUp(msg_id) => {
            exec::wake(msg_id.into()).expect("Failed to wake up the message");
        }
        Command::MxLock(continuation) => {
            let lock_guard = unsafe { MUTEX.lock().await };
            process_mx_lock_continuation(lock_guard, continuation).await;
        }
        Command::MxLockStaticAccess(subcommand) => {
            process_mx_lock_static_access_subcommand(subcommand);
        }
        Command::RwLock(lock_type, continuation) => {
            match lock_type {
                RwLockType::Read => {
                    let _lock_guard = unsafe { RW_LOCK.read().await };
                    process_rw_lock_continuation(continuation).await;
                }
                RwLockType::Write => {
                    let _lock_guard = unsafe { RW_LOCK.write().await };
                    process_rw_lock_continuation(continuation).await;
                }
            };
        }
    }
}

fn process_wait_subcommand(subcommand: WaitSubcommand) {
    match subcommand {
        WaitSubcommand::Wait => exec::wait(),
        WaitSubcommand::WaitFor(duration) => exec::wait_for(duration),
        WaitSubcommand::WaitUpTo(duration) => exec::wait_up_to(duration),
    }
}

async fn process_mx_lock_continuation(
    lock_guard: lock::MutexGuard<'static, ()>,
    continuation: MxLockContinuation,
) {
    match continuation {
        MxLockContinuation::Nothing => {}
        MxLockContinuation::SleepFor(duration) => exec::sleep_for(duration).await,
        MxLockContinuation::MoveToStatic => unsafe {
            MUTEX_LOCK_GUARD = Some(lock_guard);
        },
    }
}

fn process_mx_lock_static_access_subcommand(subcommand: MxLockStaticAccessSubcommand) {
    match subcommand {
        MxLockStaticAccessSubcommand::Drop => unsafe {
            MUTEX_LOCK_GUARD = None;
        },
        MxLockStaticAccessSubcommand::AsRef => unsafe {
            let _ = MUTEX_LOCK_GUARD.as_ref().unwrap().as_ref();
        },
        MxLockStaticAccessSubcommand::AsMut => unsafe {
            let _ = MUTEX_LOCK_GUARD.as_mut().unwrap().as_mut();
        },
        MxLockStaticAccessSubcommand::Deref => unsafe {
            let _ = MUTEX_LOCK_GUARD.as_ref().unwrap().deref();
        },
        MxLockStaticAccessSubcommand::DerefMut => unsafe {
            let _ = MUTEX_LOCK_GUARD.as_mut().unwrap().deref_mut();
        },
    }
}

async fn process_rw_lock_continuation(continuation: RwLockContinuation) {
    match continuation {
        RwLockContinuation::Nothing => {}
        RwLockContinuation::SleepFor(duration) => exec::sleep_for(duration).await,
    }
}
