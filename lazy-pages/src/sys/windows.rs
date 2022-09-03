/*
 * This file is part of Gear.
 *
 * Copyright (C) 2022 Gear Technologies Inc.
 * SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

use crate::sys::ExceptionInfo;
use std::io;
use winapi::{
    shared::ntdef::LONG,
    um::{
        errhandlingapi::SetUnhandledExceptionFilter, minwinbase::EXCEPTION_ACCESS_VIOLATION,
        winnt::EXCEPTION_POINTERS,
    },
    vc::excpt::{EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH},
};

unsafe extern "system" fn exception_handler(exception_info: *mut EXCEPTION_POINTERS) -> LONG {
    let exception_record = (*exception_info).ExceptionRecord;

    let is_access_violation = (*exception_record).ExceptionCode == EXCEPTION_ACCESS_VIOLATION;
    let num_params = (*exception_record).NumberParameters;
    if !is_access_violation || num_params != 2 {
        log::trace!(
            "Skip exception in handler: is access violation: {}, parameters: {}",
            is_access_violation,
            num_params
        );
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let addr = (*exception_record).ExceptionInformation[1];
    let is_write = match (*exception_record).ExceptionInformation[0] {
        0 /* read */ => Some(false),
        1 /* write */ => Some(true),
        // we work with WASM memory which is handled by WASM executor 
        // (e.g. it reads and writes, but don't execute as native code)
        // that's why the case is impossible
        8 /* DEP */ => unreachable!("data execution prevention"),
        _ => None,
    };
    let info = ExceptionInfo {
        fault_addr: addr as *mut _,
        is_write,
    };

    super::user_signal_handler(info)
        .map_err(|err| err.to_string())
        .expect("Memory exception handler failed");

    EXCEPTION_CONTINUE_EXECUTION
}

pub unsafe fn setup_signal_handler() -> io::Result<()> {
    SetUnhandledExceptionFilter(Some(exception_handler));
    Ok(())
}
