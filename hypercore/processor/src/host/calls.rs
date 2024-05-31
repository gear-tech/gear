// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use super::HostState;
use crate::host::utils;
use log::Level;
use wasmtime::{Caller, Memory};

fn mem_of(caller: &mut Caller<'_, HostState>) -> Memory {
    caller.get_export("memory").unwrap().into_memory().unwrap()
}

pub fn log_v1(mut caller: Caller<'_, HostState>, level: i32, target: i64, message: i64) {
    let level = match level {
        1 => Level::Error,
        2 => Level::Warn,
        3 => Level::Info,
        4 => Level::Debug,
        _ => Level::Trace,
    };

    let mem = mem_of(&mut caller);

    let target = utils::read_ri_slice(&mem, &mut caller, target);
    let target = core::str::from_utf8(&target).unwrap_or_default();

    let message = utils::read_ri_slice(&mem, &mut caller, message);
    let message = core::str::from_utf8(&message).unwrap_or_default();

    log::log!(target: target, level, "{message}");
}

pub fn max_level_v1(_: Caller<'_, HostState>) -> i32 {
    log::max_level() as usize as i32
}

pub fn program_id(mut caller: Caller<'_, HostState>, ptr: u32) {
    let program_id = caller.data().program_id;

    let mem = mem_of(&mut caller);

    mem.write(caller, ptr as usize, program_id.as_ref())
        .unwrap();
}
