// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    Command, LockContinuation, LockStaticAccessSubcommand, MxLockContinuation, RwLockContinuation,
    RwLockType, SleepForWaitType, WaitSubcommand,
};
use core::ops::{Deref, DerefMut};
use futures::future;
use gstd::{errors::Error, exec, format, lock, msg, MessageId};

static mut MUTEX: lock::Mutex<()> = lock::Mutex::new(());
static mut MUTEX_LOCK_GUARD: Option<lock::MutexGuard<()>> = None;
static mut RW_LOCK: lock::RwLock<()> = lock::RwLock::new(());
static mut R_LOCK_GUARD: Option<lock::RwLockReadGuard<()>> = None;
static mut W_LOCK_GUARD: Option<lock::RwLockWriteGuard<()>> = None;

#[gstd::async_main]
async fn main() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait(subcommand) => process_wait_subcommand(subcommand),
        Command::SendFor(to, duration) => {
            msg::send_bytes_for_reply(to.into(), [], 0, 0)
                .expect("send message failed")
                .exactly(Some(duration))
                .expect("Invalid wait duration.")
                .await;
        }
        Command::SendUpTo(to, duration) => {
            msg::send_bytes_for_reply(to.into(), [], 0, 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;
        }
        Command::SendUpToWait(to, duration) => {
            msg::send_bytes_for_reply(to.into(), [], 0, 0)
                .expect("send message failed")
                .up_to(Some(duration))
                .expect("Invalid wait duration.")
                .await;

            // after waking, wait again.
            msg::send_bytes_for_reply(to.into(), [], 0, 0)
                .expect("send message failed")
                .await;
        }
        Command::SendAndWaitFor(duration, to) => {
            msg::send(to.into(), b"ping", 0);
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
            process_mx_lock_continuation(
                unsafe { &mut MUTEX_LOCK_GUARD },
                lock_guard,
                continuation,
            )
            .await;
        }
        Command::MxLockStaticAccess(subcommand) => {
            process_lock_static_access_subcommand_mut(unsafe { &mut MUTEX_LOCK_GUARD }, subcommand);
        }
        Command::RwLock(lock_type, continuation) => {
            match lock_type {
                RwLockType::Read => {
                    let lock_guard = unsafe { RW_LOCK.read().await };
                    process_rw_lock_continuation(
                        unsafe { &mut R_LOCK_GUARD },
                        lock_guard,
                        continuation,
                    )
                    .await;
                }
                RwLockType::Write => {
                    let lock_guard = unsafe { RW_LOCK.write().await };
                    process_rw_lock_continuation(
                        unsafe { &mut W_LOCK_GUARD },
                        lock_guard,
                        continuation,
                    )
                    .await;
                }
            };
        }
        Command::RwLockStaticAccess(lock_type, subcommand) => match lock_type {
            RwLockType::Read => {
                process_lock_static_access_subcommand(unsafe { &mut R_LOCK_GUARD }, subcommand);
            }
            RwLockType::Write => {
                process_lock_static_access_subcommand_mut(unsafe { &mut W_LOCK_GUARD }, subcommand);
            }
        },
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
    static_lock_guard: &'static mut Option<lock::MutexGuard<'static, ()>>,
    lock_guard: lock::MutexGuard<'static, ()>,
    continuation: MxLockContinuation,
) {
    match continuation {
        MxLockContinuation::General(continuation) => {
            process_lock_continuation(static_lock_guard, lock_guard, continuation).await
        }
    }
}

async fn process_rw_lock_continuation<G>(
    static_lock_guard: &'static mut Option<G>,
    lock_guard: G,
    continuation: RwLockContinuation,
) {
    match continuation {
        RwLockContinuation::General(continuation) => {
            process_lock_continuation(static_lock_guard, lock_guard, continuation).await
        }
    }
}

async fn process_lock_continuation<G>(
    static_lock_guard: &'static mut Option<G>,
    lock_guard: G,
    continuation: LockContinuation,
) {
    match continuation {
        LockContinuation::Nothing => {}
        LockContinuation::SleepFor(duration) => exec::sleep_for(duration).await,
        LockContinuation::MoveToStatic => unsafe {
            *static_lock_guard = Some(lock_guard);
        },
    }
}

fn process_lock_static_access_subcommand<G, V>(
    lock_guard: &mut Option<G>,
    subcommand: LockStaticAccessSubcommand,
) where
    G: Deref + AsRef<V>,
{
    match subcommand {
        LockStaticAccessSubcommand::Drop => {
            *lock_guard = None;
        }
        LockStaticAccessSubcommand::AsRef => {
            let _ = lock_guard.as_ref().unwrap().as_ref();
        }
        LockStaticAccessSubcommand::Deref => {
            let _ = lock_guard.as_ref().unwrap().deref();
        }
        _ => unreachable!("Invalid lock static access subcommand {:?}", subcommand),
    }
}

fn process_lock_static_access_subcommand_mut<G, V>(
    lock_guard: &mut Option<G>,
    subcommand: LockStaticAccessSubcommand,
) where
    G: Deref + DerefMut + AsRef<V> + AsMut<V>,
{
    match subcommand {
        LockStaticAccessSubcommand::AsMut => {
            let _ = lock_guard.as_mut().unwrap().as_mut();
        }
        LockStaticAccessSubcommand::DerefMut => {
            let _ = lock_guard.as_mut().unwrap().deref_mut();
        }
        _ => process_lock_static_access_subcommand(lock_guard, subcommand),
    }
}
