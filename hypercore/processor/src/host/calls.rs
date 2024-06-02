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

//! TODO: impl error handling.

use crate::host::utils;
use anyhow::Result;
use log::Level;
use wasmtime::{AsContextMut, Caller, Linker, Memory};

pub mod logging {
    use super::*;

    pub fn link<T: 'static>(linker: &mut Linker<T>) -> Result<()> {
        linker.func_wrap("env", "logging_log_v1", log::<T>)?;
        linker.func_wrap("env", "logging_max_level_v1", max_level::<T>)?;

        Ok(())
    }

    fn log<C>(mut caller: Caller<'_, C>, level: i32, target: i64, message: i64) {
        let level = match level {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            _ => Level::Trace,
        };

        let mem = utils::mem_of(&mut caller);

        let target = utils::read_ri_slice(&mem, &mut caller, target);
        let target = core::str::from_utf8(&target).unwrap_or_default();

        let message = utils::read_ri_slice(&mem, &mut caller, message);
        let message = core::str::from_utf8(&message).unwrap_or_default();

        log::log!(target: target, level, "{message}");
    }

    fn max_level<C>(_: Caller<'_, C>) -> i32 {
        log::max_level() as usize as i32
    }
}

pub mod code {
    use super::*;
    use crate::host::context::CodeContext;

    pub fn link<T: 'static + CodeContext>(linker: &mut Linker<T>) -> Result<()> {
        linker.func_wrap("env", "code_load_v1", load::<T>)?;

        Ok(())
    }

    fn load<T: CodeContext>(mut caller: Caller<'_, T>, buffer_ptr: i32) {
        let mem = utils::mem_of(&mut caller);
        // TODO: set/take here to avoid mut borrowing.
        let code = caller.data().code().to_vec();

        mem.write(&mut caller, buffer_ptr as usize, code.as_ref())
            .unwrap();
    }
}
