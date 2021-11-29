// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use alloc::{string::String, vec, vec::Vec};
use gear_core::env::{Ext, LaterExt};
use gear_core::message::{MessageId, OutgoingPacket, ReplyPacket};
use gear_core::program::ProgramId;

pub fn alloc<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<u32, &'static str> {
    move |pages: i32| {
        let pages = pages as u32;

        let ptr = ext.with(|ext: &mut E| ext.alloc(pages.into()))?.map(|v| {
            let ptr = v.raw();
            log::debug!("ALLOC PAGES: {} pages at {}", pages, ptr);
            ptr
        })?;

        Ok(ptr)
    }
}

pub fn block_height<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> i32 {
    move || ext.with(|ext: &mut E| ext.block_height()).unwrap_or(0) as i32
}

pub fn block_timestamp<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> i64 {
    move || ext.with(|ext: &mut E| ext.block_timestamp()).unwrap_or(0) as i64
}

pub fn exit_code<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> Result<i32, &'static str> {
    move || {
        let reply_tuple = ext.with(|ext: &mut E| ext.reply_to())?;

        if let Some((_, exit_code)) = reply_tuple {
            Ok(exit_code)
        } else {
            Err("Not running in the reply context")
        }
    }
}

pub fn free<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |page: i32| {
        let page = page as u32;
        if let Err(e) = ext.with(|ext: &mut E| ext.free(page.into()))? {
            log::debug!("FREE PAGE ERROR: {:?}", e);
        } else {
            log::debug!("FREE PAGE: {}", page);
        }
        Ok(())
    }
}

pub fn debug<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32) -> Result<(), &'static str> {
    move |str_ptr: i32, str_len: i32| {
        let str_ptr = str_ptr as u32 as usize;
        let str_len = str_len as u32 as usize;
        ext.with_fallible(|ext: &mut E| -> Result<(), &'static str> {
            let mut data = vec![0u8; str_len];
            ext.get_mem(str_ptr, &mut data);
            match String::from_utf8(data) {
                Ok(s) => ext.debug(&s),
                Err(_) => Err("Failed to parse debug string"),
            }
        })
    }
}

pub fn gas<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |val: i32| {
        ext.with(|ext: &mut E| ext.charge_gas(val as _))?
            .map_err(|_| "Trapping: unable to report about gas used")
    }
}

pub fn gas_available<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> i64 {
    move || ext.with(|ext: &mut E| ext.gas_available()).unwrap_or(0) as i64
}

pub fn msg_id<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |msg_id_ptr: i32| {
        ext.with(|ext: &mut E| {
            let message_id = ext.message_id();
            ext.set_mem(msg_id_ptr as isize as _, message_id.as_slice());
        })
    }
}

pub fn read<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32, i32) -> Result<(), &'static str> {
    move |at: i32, len: i32, dest: i32| {
        let at = at as u32 as usize;
        let len = len as u32 as usize;
        ext.with(|ext: &mut E| {
            let msg = ext.msg().to_vec();
            ext.set_mem(dest as _, &msg[at..(at + len)]);
        })
    }
}

pub fn reply<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i64, i32, i32) -> Result<(), &'static str> {
    move |payload_ptr: i32,
          payload_len: i32,
          gas_limit: i64,
          value_ptr: i32,
          message_id_ptr: i32| {
        let result = ext.with(|ext: &mut E| -> Result<(), &'static str> {
            let payload = get_vec(ext, payload_ptr, payload_len);
            let value = get_u128(ext, value_ptr);
            let message_id =
                ext.reply(ReplyPacket::new(0, payload.into(), gas_limit as _, value))?;
            ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
            Ok(())
        })?;
        result.map_err(|_| "Trapping: unable to send reply message")
    }
}

pub fn reply_commit<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i64, i32) -> Result<(), &'static str> {
    move |message_id_ptr: i32, gas_limit: i64, value_ptr: i32| {
        let result = ext.with(|ext: &mut E| -> Result<(), &'static str> {
            let value = get_u128(ext, value_ptr);
            let message_id =
                ext.reply_commit(ReplyPacket::new(0, vec![].into(), gas_limit as _, value))?;
            ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
            Ok(())
        })?;
        result.map_err(|_| "Trapping: unable to send message")
    }
}

pub fn reply_push<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32) -> Result<(), &'static str> {
    move |payload_ptr: i32, payload_len: i32| {
        ext.with(|ext: &mut E| {
            let payload = get_vec(ext, payload_ptr, payload_len);
            ext.reply_push(&payload)
        })?
        .map_err(|_| "Trapping: unable to push payload into reply")
    }
}

pub fn reply_to<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |dest: i32| {
        let maybe_message_id = ext.with(|ext: &mut E| ext.reply_to())?;

        match maybe_message_id {
            Some((message_id, _)) => ext.with(|ext| {
                ext.set_mem(dest as isize as _, message_id.as_slice());
            })?,
            None => return Err("Not running in the reply context"),
        };

        Ok(())
    }
}

pub fn send<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i32, i64, i32, i32) -> Result<(), &'static str> {
    move |program_id_ptr: i32,
          payload_ptr: i32,
          payload_len: i32,
          gas_limit: i64,
          value_ptr: i32,
          message_id_ptr: i32| {
        let result = ext.with(|ext: &mut E| -> Result<(), &'static str> {
            let dest: ProgramId = get_id(ext, program_id_ptr).into();
            let payload = get_vec(ext, payload_ptr, payload_len);
            let value = get_u128(ext, value_ptr);
            let message_id = ext.send(OutgoingPacket::new(
                dest,
                payload.into(),
                gas_limit as _,
                value,
            ))?;
            ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
            Ok(())
        })?;
        result.map_err(|_| "Trapping: unable to send message")
    }
}

pub fn send_commit<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i32, i64, i32) -> Result<(), &'static str> {
    move |handle_ptr: i32,
          message_id_ptr: i32,
          program_id_ptr: i32,
          gas_limit: i64,
          value_ptr: i32| {
        ext.with(|ext: &mut E| -> Result<(), &'static str> {
            let dest: ProgramId = get_id(ext, program_id_ptr).into();
            let value = get_u128(ext, value_ptr);
            let message_id = ext.send_commit(
                handle_ptr as _,
                OutgoingPacket::new(dest, vec![].into(), gas_limit as _, value),
            )?;
            ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
            Ok(())
        })?
        .map_err(|_| "Trapping: unable to commit and send message")
    }
}

pub fn send_init<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> Result<i32, &'static str> {
    move || {
        let result = ext.with(|ext: &mut E| ext.send_init())?;
        result
            .map_err(|_| "Trapping: unable to init message")
            .map(|handle| handle as _)
    }
}

pub fn send_push<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32, i32) -> Result<(), &'static str> {
    move |handle_ptr: i32, payload_ptr: i32, payload_len: i32| {
        ext.with(|ext: &mut E| {
            let payload = get_vec(ext, payload_ptr, payload_len);
            ext.send_push(handle_ptr as _, &payload)
        })?
        .map_err(|_| "Trapping: unable to push payload into message")
    }
}

pub fn size<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> i32 {
    move || ext.with(|ext: &mut E| ext.msg().len() as _).unwrap_or(0)
}

pub fn source<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |source_ptr: i32| {
        ext.with(|ext: &mut E| {
            let source = ext.source();
            ext.set_mem(source_ptr as isize as _, source.as_slice());
        })
    }
}

pub fn value<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |value_ptr: i32| ext.with(|ext: &mut E| set_u128(ext, value_ptr, ext.value()))
}

pub fn wait<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> Result<(), &'static str> {
    move || {
        let _ = ext.with(|ext: &mut E| ext.wait())?;
        // Intentionally return an error to break the execution
        Err(EXIT_TRAP_STR)
    }
}

pub fn wake<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |waker_id_ptr| {
        ext.with(|ext: &mut E| {
            let waker_id: MessageId = get_id(ext, waker_id_ptr).into();
            ext.wake(waker_id)
        })?
    }
}

pub const EXIT_TRAP_STR: &str = "exit";

// Helper functions
pub fn is_exit_trap(trap: &str) -> bool {
    trap.starts_with(EXIT_TRAP_STR)
}

pub fn get_id<E: Ext>(ext: &E, ptr: i32) -> [u8; 32] {
    let mut id = [0u8; 32];
    ext.get_mem(ptr as _, &mut id);
    id
}

pub fn get_u128<E: Ext>(ext: &E, ptr: i32) -> u128 {
    let mut u128_le = [0u8; 16];
    ext.get_mem(ptr as _, &mut u128_le);
    u128::from_le_bytes(u128_le)
}

pub fn get_vec<E: Ext>(ext: &E, ptr: i32, len: i32) -> Vec<u8> {
    let mut vec = vec![0u8; len as _];
    ext.get_mem(ptr as _, &mut vec);
    vec
}

pub fn set_u128<E: Ext>(ext: &mut E, ptr: i32, val: u128) {
    ext.set_mem(ptr as _, &val.to_le_bytes());
}
