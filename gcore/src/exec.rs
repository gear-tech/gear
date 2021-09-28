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

//! Sys calls related to program execution flow.
//!
//! Provides api for low-level async implementation

use crate::MessageId;

mod sys {
    extern "C" {
        pub fn gr_gas_available() -> u64;
        pub fn gr_wait() -> !;
        pub fn gr_wake(waker_id_ptr: *const u8);
    }
}

/// Allows to fetch current value for the gas available for execution
///
/// Each execution of message processing consumes gas, both on instructions and memory allocations.
/// gas_available() returns value of the gas available for spend during current execution.
///
/// # Examples
///
/// ```
/// use gcore::{msg, exec};
///
/// // Perform work while gas_available is more then 1000
/// pub unsafe extern "C" fn handle() {
///
///     while exec::gas_available() > 1000 {
///         // do work
///     }
///
/// }
///
pub fn gas_available() -> u64 {
    unsafe { sys::gr_gas_available() }
}

/// Pause current message handle execution   
///
/// If message handle execution needs to be paused, i.e. to await for some other execution to be finished before current execution can contunue ['wait'](fn@wait) method should be used.
/// Wait finishes current message handle execution with a special result and put current message into *waiting queue* to be awaken using correponding ['wake'](fn@wake) function later.
/// All gas that is not yet spent attributed to a message in *waiting queue*.
///
/// # Examples
///
/// ```
///
/// use gcore::exec;
///
/// pub unsafe extern "C" fn handle() {
///     // do work
///     // ...
///     // pause processing
///     exec::wait();
/// }
///
pub fn wait() -> ! {
    unsafe { sys::gr_wait() }
}

/// Continue previously paused message handle execution
///
/// If message was paused using ['wait'](fn@wait) function then it is possible to continue its execution by calling ['wake'](fn@wake) function.
/// Argument *MessageId* specifies particular message to be taken out of *waiting queue* and put into *processing queue*.
///
/// # Examples
///
/// ```
/// use gcore::{exec, MessageId};
///
///
/// // Perform work while gas_available is more then 1000
/// pub unsafe extern "C" fn handle() {
///     // do work
///     exec::wake(MessageId::default());
/// }
///
pub fn wake(waker_id: MessageId) {
    unsafe {
        sys::gr_wake(waker_id.as_slice().as_ptr());
    }
}
