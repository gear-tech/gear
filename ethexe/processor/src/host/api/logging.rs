// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::MemoryWrap;
use log::Level;
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_logging_log_v1", log)?;
    linker.func_wrap("env", "ext_logging_max_level_v1", max_level)?;

    Ok(())
}

fn log(caller: Caller<'_, StoreData>, level: i32, target: i64, message: i64) {
    log::trace!(target: "host_call", "log(level={level:?}, target={target:?}, message={message:?})");

    let level = match level {
        1 => Level::Error,
        2 => Level::Warn,
        3 => Level::Info,
        4 => Level::Debug,
        _ => Level::Trace,
    };

    let memory = MemoryWrap(caller.data().memory());

    let target = memory.slice_by_val(&caller, target);
    let target = core::str::from_utf8(target).unwrap_or_default();

    let message = memory.slice_by_val(&caller, message);
    let message = core::str::from_utf8(message).unwrap_or_default();

    log::log!(target: target, level, "{message}");
}

fn max_level(_: Caller<'_, StoreData>) -> i32 {
    log::trace!(target: "host_call", "max_level()");

    let res = log::max_level() as usize as i32;

    log::trace!(target: "host_call", "max_level() -> level={res:?}");

    res
}
