// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::executor::HostState;
use wasmi::{
    core::memory_units::Pages, AsContext, AsContextMut, Caller, Extern, Func, Memory, Store,
};

pub fn alloc(store: &mut Store<HostState>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        store,
        move |mut caller: Caller<'_, HostState>, pages: i32| {
            memory
                .clone()
                .grow(caller.as_context_mut(), Pages(pages as usize))
                .map_or_else(
                    |err| {
                        log::error!("{err:?}");
                        u32::MAX as i32
                    },
                    |pages| pages.0 as i32,
                )
        },
    ))
}

pub fn free(ctx: impl AsContextMut) -> Extern {
    Extern::Func(Func::wrap(ctx, |_: i32| 0))
}

pub fn gr_panic(ctx: &mut Store<HostState>, _memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut _caller: Caller<'_, HostState>, _ptr: u32, _len: i32| {
            // let (ptr, len) = (ptr as usize, len as usize);
            //
            // let mut msg = vec![0; len];
            // memory
            //     .clone()
            //     .read(ctx.as_context(), ptr, &mut msg)
            //     .map_err(|e| {
            //         log::error!("{:?}", e);
            //         // Trap::i32_exit(1)
            //     })
            //     .unwrap();
            //
            // log::error!("panic occurred: {:?}", String::from_utf8_lossy(&msg));
            // Ok(())
        },
    ))
}

pub fn gr_oom_panic(ctx: impl AsContextMut) -> Extern {
    Extern::Func(Func::wrap(ctx, || {
        log::error!("OOM panic occurred");
        Ok(())
    }))
}

pub fn gr_read(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut caller: Caller<'_, HostState>, at: u32, len: i32, buff: i32, err: i32| {
            let (at, len, buff, err) = (at as _, len as _, buff as _, err as _);

            let msg = &caller.host_data().msg;
            let mut payload = vec![0; len];
            if at + len <= msg.len() {
                payload.copy_from_slice(&msg[at..(at + len)]);
            } else {
                log::error!("overflow");
                // return Err(Trap::i32_exit(1));
                return Ok(());
            }

            let len: u32 = memory
                .clone()
                .write(caller.as_context_mut(), buff, &payload)
                .map_err(|e| log::error!("{:?}", e))
                .is_err()
                .into();

            memory
                .clone()
                .write(caller.as_context_mut(), err, &len.to_le_bytes())
                .map_err(|e| {
                    log::error!("{:?}", e);
                    // Trap::i32_exit(1)
                })
                .unwrap();

            Ok(())
        },
    ))
}

/// # NOTE
///
/// Just for the compatibility with the program metadata
pub fn gr_reply(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut caller: Caller<'_, HostState>, ptr: u32, len: i32, _value: i32, _err: i32| {
            // TODO: process payload from here.
            let len = len as usize;
            let mut result = vec![0; len];
            memory
                .read(caller.as_context(), ptr as usize, &mut result)
                .unwrap();

            caller.host_data_mut().msg = result;
            Ok(())
        },
    ))
}

pub fn gr_size(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut caller: Caller<'_, HostState>, size_ptr: u32| {
            let size = caller.host_data().msg.len() as u32;

            memory
                .clone()
                .write(
                    caller.as_context_mut(),
                    size_ptr as usize,
                    &size.to_le_bytes(),
                )
                .map_err(|e| {
                    log::error!("{:?}", e);
                })
                .unwrap();

            Ok(())
        },
    ))
}

pub fn gr_out_of_gas(ctx: &mut Store<HostState>) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |_caller: Caller<'_, HostState>| Ok(()),
    ))
}
