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

//! Sys calls related to the program execution flow.
//!
//! Provides API for low-level async implementation.

use crate::MessageId;

mod sys {
    extern "C" {
        pub fn gr_gas_available() -> u64;
        pub fn gr_wait() -> !;
        pub fn gr_wake(waker_id_ptr: *const u8) -> !;
    }
}

/// Get the current value of the gas available for execution.
///
/// Each message processing consumes gas, both on instructions execution and
/// memory allocations. This function returns a value of the gas available for
/// spending during current execution.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// // Perform work while gas_available is more than 1000
/// pub unsafe extern "C" fn handle() {
///     while exec::gas_available() > 1000 {
///         // ...
///     }
/// }
/// ```
pub fn gas_available() -> u64 {
    unsafe { sys::gr_gas_available() }
}

/// Pause the current message handling.
///
/// If the message handling needs to be paused, i.e. to await for some other
/// execution is finished before current execution can continue, this function
/// should be used. `wait` finishes current message handle execution with a
/// special result and puts the current message into the *waiting queue* to be
/// awakened using the correspondent [`wake`] function later. All gas that
/// hasn't yet been spent is attributed to the message in the *waiting queue*.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     exec::wait();
/// }
/// ```
pub fn wait() -> ! {
    unsafe { sys::gr_wait() }
}

/// Resume previously paused message handling.
///
/// If message has been paused using the [`wait`] function, then it is possible
/// to continue its execution by calling this function. `waker_id` specifies a
/// particular message to be taken out of the *waiting queue* and put into the
/// *processing queue*.
///
/// # Examples
///
/// ```
/// use gcore::{exec, MessageId};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     exec::wake(MessageId::default());
/// }
/// ```
pub fn wake(waker_id: MessageId) -> ! {
    unsafe {
        sys::gr_wake(waker_id.as_slice().as_ptr());
    }
}
