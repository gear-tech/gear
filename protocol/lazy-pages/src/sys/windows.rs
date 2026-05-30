// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::signal::{ExceptionInfo, UserSignalHandler};
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
    // Not an access violation — not a lazy-pages page fault. Hand it back
    // to the OS exception chain without running anything the Microsoft
    // VEH contract disallows in a vectored handler (heap allocation
    // through the process heap, re-entering the SEH dispatcher, logging
    // that may take a lock the interrupted thread already holds, etc.).
    // See `PVECTORED_EXCEPTION_HANDLER` remarks for the constraint set.
    if !is_access_violation || num_params != 2 {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let addr = unsafe { (*exception_record).ExceptionInformation[1] };

    // Classify the fault before doing anything that is not safe to run from
    // an exception handler. An address outside the WASM memory lazy-pages
    // currently manages on this thread is not a lazy-pages page fault: hand
    // it straight back to the OS exception chain.
    if !crate::active_wasm_region_contains(addr) {
        return EXCEPTION_CONTINUE_SEARCH;
    }

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
        // The fault is inside managed WASM memory (classified above) but
        // `H::handle` could not service it — a lazy-pages invariant
        // violation, not a foreign fault. Panic so the backtrace points at
        // the bug.
        panic!("Signal handler failed: {err}");
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
