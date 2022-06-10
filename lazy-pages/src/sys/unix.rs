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
use nix::{
    libc::{c_void, siginfo_t},
    sys::signal,
};
use std::io::{self};

extern "C" fn handle_sigsegv(_sig: i32, info: *mut siginfo_t, _ucontext: *mut c_void) {
    unsafe {
        let addr = (*info).si_addr();
        let info = ExceptionInfo {
            fault_addr: addr as *mut _,
        };

        super::user_signal_handler(info).expect("Memory exception handler");
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

    signal::sigaction(signal, &sig_action).map_err(io::Error::from)?;

    Ok(())
}
