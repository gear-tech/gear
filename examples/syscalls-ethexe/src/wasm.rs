// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{FuzzCommand, InitConfig};
use gstd::{exec, msg, prelude::*};

static mut ECHO_DEST: Option<gstd::ActorId> = None;
static mut BIG_STATE: Option<Vec<u8>> = None;

fn big_state_byte(index: usize) -> u8 {
    index as u8
}

fn echo_dest_or(dest: [u8; 32]) -> gstd::ActorId {
    let id: gstd::ActorId = dest.into();
    if id == gstd::ActorId::zero() {
        unsafe { ECHO_DEST.unwrap_or(msg::source()) }
    } else {
        id
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let config: InitConfig = msg::load().expect("invalid init payload");
    if let Some(dest) = config.echo_dest {
        unsafe {
            ECHO_DEST = Some(dest.into());
        }
    }
    msg::reply_bytes(b"init-ok", 0).expect("failed to send init reply");
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let commands: Vec<FuzzCommand> = msg::load().expect("invalid handle payload");

    let mut replied = false;

    for cmd in commands {
        match cmd {
            FuzzCommand::CheckSize => {
                let _size = msg::size();
            }
            FuzzCommand::CheckMessageId => {
                let _mid = msg::id();
            }
            FuzzCommand::CheckProgramId => {
                let _pid = exec::program_id();
            }
            FuzzCommand::CheckSource => {
                let _src = msg::source();
            }
            FuzzCommand::CheckValue => {
                let _val = msg::value();
            }

            // ── Environment info ──────────────────────────────────────
            FuzzCommand::CheckBlockHeight => {
                let h = exec::block_height();
                assert!(h > 0, "block height must be > 0");
            }
            FuzzCommand::CheckBlockTimestamp => {
                let _ts = exec::block_timestamp();
            }
            FuzzCommand::CheckGasAvailable => {
                let gas = exec::gas_available();
                assert!(gas > 0, "gas must be > 0");
            }
            FuzzCommand::CheckValueAvailable => {
                let _val = exec::value_available();
            }
            FuzzCommand::CheckEnvVars => {
                let _vars = exec::env_vars();
            }

            FuzzCommand::SendMessage {
                dest,
                payload,
                value,
            } => {
                let target = echo_dest_or(dest);

                let _ = msg::send_bytes_delayed(target, &payload, value, 0);
            }
            FuzzCommand::SendRaw { dest, payload } => {
                let target = echo_dest_or(dest);
                let handle = msg::MessageHandle::init().expect("send_init failed");
                handle.push(&payload).expect("send_push failed");
                let _ = handle.commit_delayed(target, 0, 0);
            }
            FuzzCommand::SendInput { dest } => {
                let target = echo_dest_or(dest);
                let _ = msg::send_input_delayed(target, 0, ..msg::size(), 0);
            }

            FuzzCommand::ReplyMessage { payload, value } => {
                if !replied {
                    let _ = msg::reply_bytes(&payload, value);
                    replied = true;
                }
            }
            FuzzCommand::ReplyRaw { payload } => {
                if !replied {
                    msg::reply_push(&payload).expect("reply_push failed");
                    let _ = msg::reply_commit(0);
                    replied = true;
                }
            }
            FuzzCommand::ReplyInput => {
                if !replied {
                    let _ = msg::reply_input(0, ..msg::size());
                    replied = true;
                }
            }

            FuzzCommand::AllocAndFree { alloc_pages } => {
                let pages = alloc_pages.clamp(64, 468);
                if pages > 0 {
                    let data: Vec<u8> = vec![0xABu8; pages as usize * 65536];
                    assert!(!data.is_empty(), "allocation returned empty");
                }
            }
            FuzzCommand::MemStress { count, pattern } => {
                let pages = count.clamp(32, 468);
                if pages > 0 {
                    let size = pages as usize * 65536;
                    let mut data: Vec<u8> = vec![pattern; size];

                    for byte in data.iter() {
                        assert_eq!(*byte, pattern, "memory corruption detected");
                    }
                    // Overwrite with complement
                    let complement = !pattern;
                    for byte in data.iter_mut() {
                        *byte = complement;
                    }
                    for byte in data.iter() {
                        assert_eq!(*byte, complement, "memory corruption after overwrite");
                    }
                }
            }
            FuzzCommand::ReadBigState { chunk_size, repeat } => {
                let chunk_size = chunk_size.clamp(2048, 8192) as usize;
                let repeat = repeat.clamp(1, 4) as usize;
                let append_size = chunk_size * repeat;

                let state = unsafe {
                    let state_ptr = core::ptr::addr_of_mut!(BIG_STATE);
                    if (*state_ptr).is_none() {
                        *state_ptr = Some(Vec::new());
                    }
                    (*state_ptr).as_mut().expect("state initialization failed")
                };
                let old_len = state.len();

                state.extend((old_len..old_len + append_size).map(big_state_byte));

                let new_len = state.len();
                assert_eq!(new_len, old_len + append_size, "state append size mismatch");

                let checkpoints = [
                    old_len,
                    old_len + (append_size / 2),
                    new_len.saturating_sub(1),
                ];

                for index in checkpoints {
                    let expected = big_state_byte(index);
                    assert_eq!(state[index], expected, "state read mismatch");
                }
            }

            FuzzCommand::WaitCmd => {
                // Send ok before waiting so the loader gets a reply
                if !replied {
                    msg::reply_bytes(b"ok-wait", 0).expect("reply before wait failed");
                }
                exec::wait();
            }
            FuzzCommand::WaitForCmd(duration) => {
                let dur = duration.clamp(1, 100);
                if !replied {
                    msg::reply_bytes(b"ok-wait-for", 0).expect("reply before wait_for failed");
                }
                exec::wait_for(dur);
            }
            FuzzCommand::WaitUpToCmd(duration) => {
                let dur = duration.clamp(1, 100);
                if !replied {
                    msg::reply_bytes(b"ok-wait-up-to", 0).expect("reply before wait_up_to failed");
                }
                exec::wait_up_to(dur);
            }

            FuzzCommand::DebugMessage(data) => {
                let msg_str = core::str::from_utf8(&data).unwrap_or("non-utf8");
                gstd::debug!("fuzz-debug: {msg_str}");
            }

            FuzzCommand::Noop => {}
        }
    }

    // If we haven't replied yet, send a default "ok" reply
    if !replied {
        msg::reply_bytes(b"ok", 0).expect("failed to reply ok");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    // Record that we received a reply (for reply_to / reply_code testing)
    let reply_to = msg::reply_to();
    let reply_code = msg::reply_code();
    gstd::debug!("handle_reply: reply_to={reply_to:?}, reply_code={reply_code:?}");
}
