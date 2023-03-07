// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Utility functions related to the current execution context or program
//! execution flow.
//!
//! Wraps methods from [`gcore::exec`](https://docs.gear.rs/gcore/exec/)
//! for receiving details about the current execution and controlling it.

use crate::{common::errors::Result, ActorId, MessageId};
pub use gcore::exec::{
    block_height, block_timestamp, gas_available, leave, random, system_reserve_gas,
    value_available, wait, wait_for, wait_up_to,
};

/// Terminate the execution of a program.
///
/// The program and all corresponding data are removed from the storage. It may
/// be called in the `init` method as well. One can consider this function as
/// some analog of `std::process::exit`.
///
/// `inheritor_id` specifies the address to which all available program value
/// should be transferred.
///
/// # Examples
///
/// Terminate the program and transfer the available value to the message
/// sender:
///
/// ```
/// use gstd::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // ...
///     exec::exit(msg::source());
/// }
/// ```
pub fn exit(inheritor_id: ActorId) -> ! {
    gcore::exec::exit(inheritor_id.into())
}

/// Resume previously paused message handling.
///
/// Suppose a message has been paused using the [`wait`] function,
/// it is possible to continue its execution by calling this function.
///
/// `message_id` specifies a particular message to be taken out of the *waiting
/// queue* and put into the *processing queue*.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg, MessageId};
///
/// static mut MSG_ID: MessageId = MessageId::zero();
///
/// #[no_mangle]
/// extern "C" fn init() {
///     unsafe { MSG_ID = msg::id() };
///     exec::wait();
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     exec::wake(unsafe { MSG_ID }).expect("Unable to wake");
/// }
/// ```
pub fn wake(message_id: MessageId) -> Result<()> {
    wake_delayed(message_id, 0)
}

/// Same as [`wake`], but executes after the `delay` expressed in block count.
pub fn wake_delayed(message_id: MessageId, delay: u32) -> Result<()> {
    gcore::exec::wake_delayed(message_id.into(), delay).map_err(Into::into)
}

/// Return the identifier of the current program.
///
/// # Examples
///
/// ```
/// use gstd::{exec, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let whoami = exec::program_id();
/// }
/// ```
pub fn program_id() -> ActorId {
    gcore::exec::program_id().into()
}

/// Return the identifier of the original user who initiated communication with
/// the blockchain, during which the present processing message was created.
///
/// # Examples
///
/// ```
/// use gstd::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let user = exec::origin();
/// }
/// ```
pub fn origin() -> ActorId {
    gcore::exec::origin().into()
}
