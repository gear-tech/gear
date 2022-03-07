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

use alloc::{string::String, vec};
use gear_backend_common::funcs::*;
use gear_backend_common::{EXIT_TRAP_STR, LEAVE_TRAP_STR, WAIT_TRAP_STR};
use gear_core::{
    env::{Ext, LaterExt},
    message::{MessageId, OutgoingPacket, ProgramInitPacket, ReplyPacket},
    program::ProgramId,
};
use wasmtime::{Func, Store, Trap};

pub struct FuncsHandler<E: Ext + 'static, T> {
    _unneed1: E,
    _unneed2: T,
}

impl<E: Ext + 'static, T> FuncsHandler<E, T> {
    pub fn alloc(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let f = move |pages: i32| {
            let pages = pages as u32;
            let ptr = ext
                .with(|ext| ext.alloc(pages.into()))
                .map_err(Trap::new)?
                .map_err(Trap::new)?;
            log::debug!("ALLOC PAGES: {} pages at {}", pages, ptr.raw());
            Ok(ptr.raw())
        };
        Func::wrap(store, f)
    }

    pub fn block_height(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let f = move || ext.with(|ext: &mut E| ext.block_height()).unwrap_or(0) as i32;
        Func::wrap(store, f)
    }

    pub fn block_timestamp(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let f = move || ext.with(|ext: &mut E| ext.block_timestamp()).unwrap_or(0) as i64;
        Func::wrap(store, f)
    }

    pub fn exit_code(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let f = move || {
            let reply_tuple = ext.with(|ext: &mut E| ext.reply_to()).map_err(Trap::new)?;
            if let Some((_, exit_code)) = reply_tuple {
                Ok(exit_code)
            } else {
                Err(Trap::new("Not running in the reply context"))
            }
        };
        Func::wrap(store, f)
    }

    pub fn free(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |page: i32| {
            let page = page as u32;
            ext.with(|ext: &mut E| ext.free(page.into()))
                .map_err(Trap::new)?.map_err(Trap::new)?;
            if let Err(e) = ext
                .with(|ext: &mut E| ext.free(page.into()))
                .map_err(Trap::new)?
            {
                log::debug!("FREE PAGE ERROR: {:?}", e);
            } else {
                log::debug!("FREE PAGE: {}", page);
            }
            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn debug(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let f = move |str_ptr: i32, str_len: i32| {
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
            .map_err(Trap::new)
        };
        Func::wrap(store, f)
    }

    pub fn gas(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |val: i32| {
            ext.with(|ext: &mut E| ext.charge_gas(val as _))
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to report about gas used")
                .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn gas_available(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move || ext.with(|ext: &mut E| ext.gas_available()).unwrap_or(0) as i64;
        Func::wrap(store, func)
    }

    pub fn exit(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |program_id_ptr: i32| -> Result<(), Trap> {
            ext.with(|ext: &mut E| {
                let value_dest: ProgramId = get_bytes32(ext, program_id_ptr as u32 as _).into();
                ext.exit(value_dest)
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)?;

            // Intentionally return an error to break the execution
            Err(Trap::new(EXIT_TRAP_STR))
        };
        Func::wrap(store, func)
    }

    pub fn origin(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |origin_ptr: i32| {
            ext.with(|ext: &mut E| {
                let id = ext.origin();
                ext.set_mem(origin_ptr as _, id.as_slice());
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn msg_id(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |msg_id_ptr: i32| {
            ext.with(|ext: &mut E| {
                let message_id = ext.message_id();
                ext.set_mem(msg_id_ptr as isize as _, message_id.as_slice());
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn read(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |at: i32, len: i32, dest: i32| {
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            ext.with(|ext: &mut E| {
                let msg = ext.msg().to_vec();
                ext.set_mem(dest as _, &msg[at..(at + len)]);
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func =
            move |payload_ptr: i32, payload_len: i32, value_ptr: i32, message_id_ptr: i32| {
                ext.with(|ext: &mut E| -> Result<(), &'static str> {
                    let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                    let value = get_u128(ext, value_ptr as usize);
                    let message_id = ext.reply(ReplyPacket::new(0, payload.into(), value))?;
                    ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to send reply message")
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn reply_commit(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |message_id_ptr: i32, value_ptr: i32| {
            ext.with(|ext: &mut E| -> Result<(), &'static str> {
                let value = get_u128(ext, value_ptr as usize);
                let message_id = ext.reply_commit(ReplyPacket::new(0, vec![].into(), value))?;
                ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                Ok(())
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_push(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |payload_ptr: i32, payload_len: i32| {
            ext.with(|ext: &mut E| {
                let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                ext.reply_push(&payload)
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to push payload into reply")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_to(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |dest: i32| {
            let maybe_message_id = ext.with(|ext: &mut E| ext.reply_to()).map_err(Trap::new)?;
            match maybe_message_id {
                Some((message_id, _)) => ext
                    .with(|ext| {
                        ext.set_mem(dest as isize as _, message_id.as_slice());
                    })
                    .map_err(Trap::new)?,
                None => return Err(Trap::new("Not running in the reply context")),
            };
            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn send(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            ext.with(|ext: &mut E| -> Result<(), &'static str> {
                let dest: ProgramId = get_bytes32(ext, program_id_ptr as usize).into();
                let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                let value = get_u128(ext, value_ptr as usize);
                let message_id =
                    ext.send(OutgoingPacket::new(dest, payload.into(), None, value))?;
                ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                Ok(())
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_wgas(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            ext.with(|ext: &mut E| -> Result<(), &'static str> {
                let dest: ProgramId = get_bytes32(ext, program_id_ptr as usize).into();
                let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                let value = get_u128(ext, value_ptr as usize);
                let message_id = ext.send(OutgoingPacket::new(
                    dest,
                    payload.into(),
                    Some(gas_limit as _),
                    value,
                ))?;
                ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                Ok(())
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_commit(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func =
            move |handle_ptr: i32, message_id_ptr: i32, program_id_ptr: i32, value_ptr: i32| {
                ext.with(|ext: &mut E| -> Result<(), &'static str> {
                    let dest: ProgramId = get_bytes32(ext, program_id_ptr as usize).into();
                    let value = get_u128(ext, value_ptr as usize);
                    let message_id = ext.send_commit(
                        handle_ptr as _,
                        OutgoingPacket::new(dest, vec![].into(), None, value),
                    )?;
                    ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to commit and send message")
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn send_commit_wgas(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         gas_limit: i64,
                         value_ptr: i32| {
            ext.with(|ext: &mut E| -> Result<(), &'static str> {
                let dest: ProgramId = get_bytes32(ext, program_id_ptr as usize).into();
                let value = get_u128(ext, value_ptr as usize);
                let message_id = ext.send_commit(
                    handle_ptr as _,
                    OutgoingPacket::new(dest, vec![].into(), Some(gas_limit as _), value),
                )?;
                ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                Ok(())
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to commit and send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_init(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move || {
            ext.with(|ext: &mut E| ext.send_init())
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to init message")
                .map_err(Trap::new)
                .map(|handle| handle as i32)
        };
        Func::wrap(store, func)
    }

    pub fn send_push(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |handle_ptr: i32, payload_ptr: i32, payload_len: i32| {
            ext.with(|ext: &mut E| {
                let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                ext.send_push(handle_ptr as _, &payload)
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to push payload into message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn create_program_wgas(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         program_id_ptr: i32| {
            ext.with(|ext: &mut E| -> Result<(), &'static str> {
                let code_hash = get_bytes32(ext, code_hash_ptr as usize);
                let salt = get_vec(ext, salt_ptr as usize, salt_len as usize);
                let payload = get_vec(ext, payload_ptr as usize, payload_len as usize);
                let value = get_u128(ext, value_ptr as usize);
                let new_actor_id = ext.create_program(ProgramInitPacket::new(
                    code_hash.into(),
                    salt,
                    payload.into(),
                    gas_limit as u64,
                    value,
                ))?;
                ext.set_mem(program_id_ptr as isize as _, new_actor_id.as_slice());
                Ok(())
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to create a new program")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn size(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move || ext.with(|ext: &mut E| ext.msg().len() as _).unwrap_or(0);
        Func::wrap(store, func)
    }

    pub fn source(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |source_ptr: i32| {
            ext.with(|ext: &mut E| {
                let source = ext.source();
                ext.set_mem(source_ptr as _, source.as_slice());
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn program_id(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |source_ptr: i32| {
            ext.with(|ext: &mut E| {
                let actor_id = ext.program_id();
                ext.set_mem(source_ptr as _, actor_id.as_slice());
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |value_ptr: i32| {
            ext.with(|ext: &mut E| set_u128(ext, value_ptr as usize, ext.value()))
                .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value_available(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |value_ptr: i32| {
            ext.with(|ext: &mut E| set_u128(ext, value_ptr as usize, ext.value_available()))
                .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn leave(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move || -> Result<(), Trap> {
            let _ = ext.with(|ext: &mut E| ext.leave()).map_err(Trap::new)?;
            // Intentionally return an error to break the execution
            Err(Trap::new(LEAVE_TRAP_STR))
        };
        Func::wrap(store, func)
    }

    pub fn wait(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move || -> Result<(), Trap> {
            let _ = ext.with(|ext: &mut E| ext.wait()).map_err(Trap::new)?;
            // Intentionally return an error to break the execution
            Err(Trap::new(WAIT_TRAP_STR))
        };
        Func::wrap(store, func)
    }

    pub fn wake(ext: LaterExt<E>, store: &mut Store<T>) -> Func {
        let func = move |waker_id_ptr: i32| {
            ext.with(|ext: &mut E| {
                let waker_id: MessageId = get_bytes32(ext, waker_id_ptr as usize).into();
                ext.wake(waker_id)
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }
}
