// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::{common::errors::Result, ActorId, MessageId};

/// Provide gas deposit from current message to handle reply message on given
/// message id.
///
/// This message id should be sent within the execution. Once destination actor
/// or system sends reply on it, the gas limit ignores, if the program gave
/// deposit - the only it will be used for execution of `handle_reply`.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let message_id =
///         msg::send(msg::source(), b"Outgoing message", 0).expect("Failed to send message");
///
///     exec::reply_deposit(message_id, 100_000).expect("Failed to deposit reply");
/// }
///
/// #[no_mangle]
/// extern "C" fn handle_reply() {
///     // I will be executed for pre-defined (deposited) 100_000 of gas!
/// }
/// ```
pub fn reply_deposit(message_id: MessageId, amount: u64) -> Result<()> {
    gcore::exec::reply_deposit(message_id.into(), amount).map_err(Into::into)
}

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
/// Suppose a message has been paused using the [`crate::exec::wait`] function,
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

/// Pay specified rent for the program. The result contains the remainder of
/// rent value and the count of paid blocks.
///
/// # Examples
///
/// ```
/// use gstd::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let (_unused_value, paid_block_count) =
///         exec::pay_program_rent(exec::program_id(), 1_000_000).expect("Unable to pay rent");
/// }
/// ```
pub fn pay_program_rent(program_id: ActorId, value: u128) -> Result<(u128, u32)> {
    Ok(gcore::exec::pay_program_rent(program_id.into(), value)?)
}

/// Return the identifier of the current program.
///
/// # Examples
///
/// ```
/// use gstd::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let whoami = exec::program_id();
/// }
/// ```
pub fn program_id() -> ActorId {
    gcore::exec::program_id().into()
}
