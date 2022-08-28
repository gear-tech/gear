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

use crate::{sys::ExceptionInfo, Error};
use nix::{
    libc::{c_void, siginfo_t},
    sys::{signal, signal::SigHandler},
};
use std::{cell::RefCell, io};

thread_local! {
    static OLD_SIG_HANDLER: RefCell<SigHandler> = RefCell::new(SigHandler::SigDfl);
}

extern "C" fn handle_sigsegv(sig: i32, info: *mut siginfo_t, ucontext: *mut c_void) {
    unsafe {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let is_write = {
            let ucontext = ucontext as *const nix::libc::ucontext_t;
            let error_reg = nix::libc::REG_ERR as usize;
            let error_code = (*ucontext).uc_mcontext.gregs[error_reg];
            // Use second bit from err reg. See https://git.io/JEQn3
            Some(error_code & 0b10 == 0b10)
        };

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let is_write = {
            let _unused_warning_resolver = ucontext;
            None
        };

        let addr = (*info).si_addr();
        let exc_info = ExceptionInfo {
            fault_addr: addr as *mut _,
            is_write,
        };

        if let Err(err) = super::user_signal_handler(exc_info) {
            let old_sig_handler_works = if let Error::SignalFromUnknownMemory { .. } = err {
                old_sig_handler(sig, info, ucontext)
            } else {
                false
            };
            if !old_sig_handler_works {
                panic!("Signal handler failed: {}", err);
            }
        }
    }
}

pub unsafe fn setup_signal_handler() -> io::Result<()> {
    let handler = signal::SigHandler::SigAction(handle_sigsegv);
    let sig_action = signal::SigAction::new(
        handler,
        signal::SaFlags::SA_SIGINFO,
        signal::SigSet::empty(),
    );

    let signal = if cfg!(target_os = "macos") {
        signal::SIGBUS
    } else {
        signal::SIGSEGV
    };

    let old_sigaction = signal::sigaction(signal, &sig_action).map_err(io::Error::from)?;
    OLD_SIG_HANDLER.with(|sh| *sh.borrow_mut() = old_sigaction.handler());

    Ok(())
}

unsafe fn old_sig_handler(sig: i32, info: *mut siginfo_t, ucontext: *mut c_void) -> bool {
    match OLD_SIG_HANDLER.with(|sh| *sh.borrow()) {
        SigHandler::SigDfl | SigHandler::SigIgn => false,
        SigHandler::Handler(func) => {
            func(sig);
            true
        }
        SigHandler::SigAction(func) => {
            func(sig, info, ucontext);
            true
        }
    }
}
