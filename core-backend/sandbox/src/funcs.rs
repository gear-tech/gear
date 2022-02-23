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

use crate::env::Runtime;
use alloc::{string::String, vec};
use core::{
    convert::{TryFrom, TryInto},
    slice::Iter,
};
use gear_backend_common::{funcs, ExtInfo, EXIT_TRAP_STR, LEAVE_TRAP_STR, WAIT_TRAP_STR};
use gear_core::{
    env::Ext,
    message::{MessageId, OutgoingPacket, ProgramInitPacket, ReplyPacket},
    program::ProgramId,
};
use sp_sandbox::{HostError, ReturnValue, Value};

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

pub(crate) fn pop_i32<T: TryFrom<i32>>(arg: &mut Iter<'_, Value>) -> Result<T, HostError> {
    match arg.next() {
        Some(Value::I32(val)) => Ok((*val).try_into().map_err(|_| HostError)?),
        _ => Err(HostError),
    }
}

pub(crate) fn pop_i64<T: TryFrom<i64>>(arg: &mut Iter<'_, Value>) -> Result<T, HostError> {
    match arg.next() {
        Some(Value::I64(val)) => Ok((*val).try_into().map_err(|_| HostError)?),
        _ => Err(HostError),
    }
}

pub(crate) fn return_none() -> SyscallOutput {
    Ok(ReturnValue::Unit)
}

pub(crate) fn return_i32<T: TryInto<i32>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| ReturnValue::Value(Value::I32(v)))
        .map_err(|_| HostError)
}

pub(crate) fn return_i64<T: TryInto<i64>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| ReturnValue::Value(Value::I64(v)))
        .map_err(|_| HostError)
}

pub fn send<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let program_id_ptr = pop_i32(&mut args)?;
    let payload_ptr = pop_i32(&mut args)?;
    let payload_len = pop_i32(&mut args)?;
    let gas_limit = pop_i64(&mut args)?;
    let value_ptr = pop_i32(&mut args)?;
    let message_id_ptr = pop_i32(&mut args)?;

    let result = ctx
        .ext
        .with(|ext| {
            let dest: ProgramId = funcs::get_bytes32(ext, program_id_ptr).into();
            let payload = funcs::get_vec(ext, payload_ptr, payload_len);
            let value = funcs::get_u128(ext, value_ptr);
            let message_id =
                ext.send(OutgoingPacket::new(dest, payload.into(), gas_limit, value))?;
            ext.set_mem(message_id_ptr, message_id.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_err| {
            ctx.trap = Some("Trapping: unable to send message");
            HostError
        });
    result
}

pub fn send_commit<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let handle_ptr = pop_i32(&mut args)?;
    let message_id_ptr = pop_i32(&mut args)?;
    let program_id_ptr = pop_i32(&mut args)?;
    let gas_limit = pop_i64(&mut args)?;
    let value_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let dest: ProgramId = funcs::get_bytes32(ext, program_id_ptr).into();
            let value = funcs::get_u128(ext, value_ptr);
            let message_id = ext.send_commit(
                handle_ptr,
                OutgoingPacket::new(dest, vec![].into(), gas_limit, value),
            )?;
            ext.set_mem(message_id_ptr, message_id.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_err| {
            ctx.trap = Some("Trapping: unable to send message");
            HostError
        })
}

pub fn send_init<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
    ctx.ext
        .with(|ext| ext.send_init())
        .and_then(|res| res.map(|handle| ReturnValue::Value(Value::I32(handle as i32))))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn send_push<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let handle_ptr = pop_i32(&mut args)?;
    let payload_ptr = pop_i32(&mut args)?;
    let payload_len = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let payload = funcs::get_vec(ext, payload_ptr, payload_len);
            ext.send_push(handle_ptr, &payload)
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn read<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let at = pop_i32(&mut args)?;
    let len: usize = pop_i32(&mut args)?;
    let dest = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let msg = ext.msg().to_vec();
            ext.set_mem(dest, &msg[at..(at + len)]);
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn size<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
    ctx.ext
        .with(|ext| ext.msg().len())
        .map(return_i32)
        .unwrap_or_else(|_| return_i32(0))
}

pub fn exit<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let value_dest_ptr = pop_i32(&mut args.iter())?;

    let _: Result<ReturnValue, HostError> = ctx
        .ext
        .with(|ext: &mut E| {
            let value_dest: ProgramId = funcs::get_bytes32(ext, value_dest_ptr).into();
            ext.exit(value_dest)
        })
        .map(|_| Ok(ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })?;

    ctx.trap = Some(EXIT_TRAP_STR);
    Err(HostError)
}

pub fn exit_code<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
    let reply_tuple = ctx.ext.with(|ext| ext.reply_to()).map_err(|err| {
        ctx.trap = Some(err);
        HostError
    })?;

    if let Some((_, exit_code)) = reply_tuple {
        return_i32(exit_code)
    } else {
        ctx.trap = Some("Trapping: exit code ran into non-reply scenario");
        Err(HostError)
    }
}

pub fn gas<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let val = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| ext.charge_gas(val))
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_err| {
            ctx.trap = Some("Trapping: unable to report about gas used");
            HostError
        })
}

pub fn alloc<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let pages: u32 = pop_i32(&mut args)?;

    let ptr = ctx.ext.with(|ext| ext.alloc(pages.into())).and_then(|v| {
        v.map(|v| {
            let ptr = v.raw();
            log::debug!("ALLOC: {} pages at {}", pages, ptr);
            ptr
        })
    });

    ptr.map(|res| ReturnValue::Value(Value::I32(res as i32)))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn free<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let page: u32 = pop_i32(&mut args)?;

    if let Err(e) = ctx.ext.with(|ext| ext.free(page.into())).map_err(|err| {
        ctx.trap = Some(err);
        HostError
    })? {
        log::debug!("FREE ERROR: {:?}", e);
    } else {
        log::debug!("FREE: {}", page);
    }

    return_none()
}

pub fn block_height<E: Ext + Into<ExtInfo>>(
    ctx: &mut Runtime<E>,
    _args: &[Value],
) -> SyscallOutput {
    let block_height = ctx
        .ext
        .with(|ext| ext.block_height())
        .map_err(|_| HostError)?;

    return_i32(block_height)
}

pub fn block_timestamp<E: Ext + Into<ExtInfo>>(
    ctx: &mut Runtime<E>,
    _args: &[Value],
) -> SyscallOutput {
    let block_timestamp = ctx
        .ext
        .with(|ext| ext.block_timestamp())
        .map_err(|_| HostError)?;

    return_i64(block_timestamp)
}

pub fn reply<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let payload_ptr = pop_i32(&mut args)?;
    let payload_len = pop_i32(&mut args)?;
    let gas_limit = pop_i64(&mut args)?;
    let value_ptr = pop_i32(&mut args)?;

    let result = ctx
        .ext
        .with(|ext| {
            let payload = funcs::get_vec(ext, payload_ptr, payload_len);
            let value = funcs::get_u128(ext, value_ptr);
            ext.reply(ReplyPacket::new(0, payload.into(), gas_limit, value))
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_| {
            ctx.trap = Some("Trapping: unable to send reply message");
            HostError
        });
    result
}

pub fn reply_commit<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let message_id_ptr = pop_i32(&mut args)?;
    let gas_limit = pop_i64(&mut args)?;
    let value_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let value = funcs::get_u128(ext, value_ptr);
            let message_id =
                ext.reply_commit(ReplyPacket::new(0, vec![].into(), gas_limit, value))?;
            ext.set_mem(message_id_ptr, message_id.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_| {
            ctx.trap = Some("Trapping: unable to send message");
            HostError
        })
}

pub fn reply_to<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let dest = pop_i32(&mut args)?;

    let maybe_message_id = ctx.ext.with(|ext| ext.reply_to()).map_err(|err| {
        ctx.trap = Some(err);
        HostError
    })?;

    match maybe_message_id {
        Some((message_id, _)) => ctx
            .ext
            .with(|ext| {
                ext.set_mem(dest, message_id.as_slice());
            })
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?,
        None => {
            ctx.trap = Some("Not running in the reply context");
            return Err(HostError);
        }
    };

    return_none()
}

pub fn reply_push<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let payload_ptr = pop_i32(&mut args)?;
    let payload_len = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let payload = funcs::get_vec(ext, payload_ptr, payload_len);
            ext.reply_push(&payload)
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_| {
            ctx.trap = Some("Trapping: unable to push payload into reply");
            HostError
        })
}

pub fn debug<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let str_ptr = pop_i32(&mut args)?;
    let str_len = pop_i32(&mut args)?;

    ctx.ext
        .with_fallible(|ext| {
            let mut data = vec![0u8; str_len];
            ext.get_mem(str_ptr, &mut data);
            match String::from_utf8(data) {
                Ok(s) => ext.debug(&s),
                Err(_) => Err("Failed to parse debug string"),
            }
        })
        .map(|_| ReturnValue::Unit)
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn gas_available<E: Ext + Into<ExtInfo>>(
    ctx: &mut Runtime<E>,
    _args: &[Value],
) -> SyscallOutput {
    let gas_available = ctx
        .ext
        .with(|ext| ext.gas_available())
        .map_err(|_| HostError)?;

    return_i64(gas_available)
}

pub fn msg_id<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let msg_id_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let message_id = ext.message_id();
            ext.set_mem(msg_id_ptr, message_id.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn program_id<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let source_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let source = ext.program_id();
            ext.set_mem(source_ptr, source.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn source<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let source_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let source = ext.source();
            ext.set_mem(source_ptr, source.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn value<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let value_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            funcs::set_u128(ext, value_ptr, ext.value());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn value_available<E: Ext + Into<ExtInfo>>(
    ctx: &mut Runtime<E>,
    args: &[Value],
) -> SyscallOutput {
    let mut args = args.iter();

    let value_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            funcs::set_u128(ext, value_ptr, ext.value_available());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
}

pub fn leave<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
    let _: Result<ReturnValue, HostError> = ctx
        .ext
        .with(|ext| ext.leave())
        .map(|_| return_none())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })?;
    ctx.trap = Some(LEAVE_TRAP_STR);
    Err(HostError)
}

pub fn wait<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
    let _: Result<ReturnValue, HostError> = ctx
        .ext
        .with(|ext| ext.wait())
        .map(|_| return_none())
        .map_err(|err| {
        ctx.trap = Some(err);
        HostError
    })?;
    ctx.trap = Some(WAIT_TRAP_STR);
    Err(HostError)
}

pub fn wake<E: Ext + Into<ExtInfo>>(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
    let mut args = args.iter();

    let waker_id_ptr = pop_i32(&mut args)?;

    ctx.ext
        .with(|ext| {
            let waker_id: MessageId = funcs::get_bytes32(ext, waker_id_ptr).into();
            ext.wake(waker_id)
        })
        .map(|_| return_none())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })?
}

pub fn create_program<E: Ext + Into<ExtInfo>>(
    ctx: &mut Runtime<E>,
    args: &[Value],
) -> Result<ReturnValue, HostError> {
    let mut args = args.iter();

    let code_hash_ptr = pop_i32(&mut args)?;
    let salt_ptr = pop_i32(&mut args)?;
    let salt_len = pop_i32(&mut args)?;
    let payload_ptr = pop_i32(&mut args)?;
    let payload_len = pop_i32(&mut args)?;
    let gas_limit = pop_i64(&mut args)?;
    let value_ptr = pop_i32(&mut args)?;
    let program_id_ptr = pop_i32(&mut args)?;

    let result = ctx
        .ext
        .with(|ext: &mut E| -> Result<(), &'static str> {
            let code_hash = funcs::get_bytes32(ext, code_hash_ptr);
            let salt = funcs::get_vec(ext, salt_ptr, salt_len);
            let payload = funcs::get_vec(ext, payload_ptr, payload_len);
            let value = funcs::get_u128(ext, value_ptr);
            let new_actor_id = ext.create_program(ProgramInitPacket::new(
                code_hash.into(),
                salt,
                payload.into(),
                gas_limit,
                value,
            ))?;
            ext.set_mem(program_id_ptr, new_actor_id.as_slice());
            Ok(())
        })
        .and_then(|res| res.map(|_| ReturnValue::Unit))
        .map_err(|_err| {
            ctx.trap = Some("Trapping: unable to create program");
            HostError
        });
    result
}
