/*
 * This file is part of Gear.
 *
 * Copyright (C) 2022-2025 Gear Technologies Inc.
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

use crate::{
    common::Error,
    signal::{ExceptionInfo, UserSignalHandler},
};
use std::io;
use winapi::{
    shared::ntdef::LONG,
    um::{
        errhandlingapi::AddVectoredExceptionHandler, minwinbase::EXCEPTION_ACCESS_VIOLATION,
        winnt::EXCEPTION_POINTERS,
    },
    vc::excpt::{EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH},
};

unsafe extern "system" fn exception_handler<H>(exception_info: *mut EXCEPTION_POINTERS) -> LONG
where
    H: UserSignalHandler,
{
    let exception_record = unsafe { (*exception_info).ExceptionRecord };
    check_windows_stack();

    let is_access_violation =
        unsafe { (*exception_record).ExceptionCode == EXCEPTION_ACCESS_VIOLATION };
    let num_params = unsafe { (*exception_record).NumberParameters };
    if !is_access_violation || num_params != 2 {
        log::trace!(
            "Skip exception in handler: is access violation: {}, parameters: {}",
            is_access_violation,
            num_params
        );
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let addr = unsafe { (*exception_record).ExceptionInformation[1] };
    let is_write = match unsafe { (*exception_record).ExceptionInformation[0] } {
        0 /* read */ => Some(false),
        1 /* write */ => Some(true),
        // we work with WASM memory which is handled by WASM executor
        // (e.g. it reads and writes, but doesn't execute as native code)
        // that's why the case is impossible
        8 /* DEP */ => {
            let err_msg = "exception_handler: data execution prevention.";

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        }
        // existence of other values is undocumented and I expect they should be treated as reserved
        _ => None,
    };
    let info = ExceptionInfo {
        fault_addr: addr as *mut _,
        is_write,
    };

    if let Err(err) = unsafe { H::handle(info) } {
        check_windows_stack();
        if let Error::OutOfWasmMemoryAccess | Error::WasmMemAddrIsNotSet = err {
            return EXCEPTION_CONTINUE_SEARCH;
        } else {
            panic!("Signal handler failed: {err}");
        }
    }

    check_windows_stack();

    EXCEPTION_CONTINUE_EXECUTION
}

pub(crate) unsafe fn init_for_thread() -> Result<(), String> {
    Ok(())
}

pub(crate) unsafe fn setup_signal_handler<H>() -> io::Result<()>
where
    H: UserSignalHandler,
{
    const CALL_FIRST: bool = true;

    let handle =
        unsafe { AddVectoredExceptionHandler(CALL_FIRST as _, Some(exception_handler::<H>)) };
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(debug_assertions)]
#[inline(never)]
fn check_windows_stack() {
    use std::arch::asm;

    // Simple sanity check for the stack limit on Windows.
    // This is a debug assertion to ensure that
    // the stack pointer (rsp) is not below the stack limit.

    let stack_limit: u64;
    let rsp: u64;

    unsafe {
        asm!(
            "mov {}, gs:[0x10]",   // stack limit
            "mov {}, rsp",
            out(reg) stack_limit,
            out(reg) rsp,
        );
    }

    if rsp < stack_limit {
        eprintln!("***********rsp {rsp:#X} < stack_limit!!! {stack_limit:#X}");
        std::process::exit(13);
    }
}

#[cfg(not(debug_assertions))]
fn check_windows_stack() {}
