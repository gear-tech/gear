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

//! Sys calls related to the program execution flow.
//!
//! Wraps methods from `gcore` for getting the current block height,
//! the current block timestamp, the current value of the gas available for
//! execution.
//!
//!
//! The block height serves to identify a particular block.
//! This information can be used to enable many scenarios like restricting or
//! allowing of some functions until certain block height is reached.
//!
//! The timestamp is the number of milliseconds elapsed since the Unix epoch.
//!
//! Each message processing consumes gas, both on instructions execution and
//! memory allocations. This function returns a value of the gas available for
//! spending during current execution. Its use may help to avoid unexpected
//! behaviors during the smart-contract execution in case of not enough gas
//! available.
//!
//! Value available is the total available value of program.
//!
//! # Examples
//!
//! ```
//! use gstd::{exec, msg};
//!
//! // Send a reply after the block height reaches the number 1000
//! unsafe extern "C" fn handle() {
//!     if exec::block_height() >= 1000 {
//!         msg::reply(b"Block #1000 reached", 0).unwrap();
//!     }
//! }
//! ```
//! ```
//! use gstd::{exec, msg};
//!
//! // Send a reply after the block timestamp reaches the February 22, 2022
//! unsafe extern "C" fn handle() {
//!     if exec::block_timestamp() >= 1645488000000 {
//!         msg::reply(b"The current block is generated after February 22, 2022", 0).unwrap();
//!     }
//! }
//! ```
//! ```
//! use gstd::exec;
//!
//! // Perform work while `gas_available` is more than 1000
//! unsafe extern "C" fn handle() {
//!     while exec::gas_available() > 1000 {
//!         // ...
//!     }
//! }
//! ```
//! ```
//! use gstd::exec;
//!
//! // Get self value balance in program
//! unsafe extern "C" fn handle() {
//!     let _my_balance = exec::value_available();
//! }
//! ```
use crate::{ActorId, MessageId};
pub use gcore::exec::{block_height, block_timestamp, gas_available, value_available};

/// Terminate the execution of a program. The program and all corresponding data
/// are removed from the storage. This is similiar to
/// `std::process::exit`. `value_destination` specifies the address where all
/// available to the program value should be transferred to.
/// Maybe called in `init` method as well.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::exit(msg::source());
/// }
/// ```
pub fn exit(value_destination: ActorId) -> ! {
    gcore::exec::exit(value_destination.into())
}

/// Terminate the current message handling.
///
/// For cases when the message handling needs to be terminated with state
/// saving.
///
/// # Examples
///
/// ```
/// use gstd::exec;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::leave();
/// }
/// ```
pub fn leave() -> ! {
    gcore::exec::leave()
}

/// Pause the current message handling.
///
/// If the message handling needs to be paused, i.e. to wait for another
/// execution to finish, this function should be used. [`wait`] finishes current
/// message handle execution with a special result and puts the current message
/// into the *waiting queue* to be awakened using the correspondent [`wake`]
/// function later. All gas that hasn't yet been spent is attributed to the
/// message in the *waiting queue*.
///
/// # Examples
///
/// ```
/// use gstd::exec;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::wait();
/// }
/// ```
pub fn wait() -> ! {
    gcore::exec::wait()
}

/// Resume previously paused message handling.
///
/// If a message has been paused using the [`wait`] function, then it is
/// possible to continue its execution by calling this function. `waker_id`
/// which specifies a particular message to be taken out of the *waiting queue*
/// and put into the *processing queue*.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let msg_id = msg::id();
///     exec::wake(msg_id);
/// }
/// ```
pub fn wake(waker_id: MessageId) {
    gcore::exec::wake(waker_id.into())
}

/// Return ID of the current program.
///
/// # Examples
///
/// ```
/// use gstd::{exec, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let me = exec::program_id();
/// }
/// ```
pub fn program_id() -> ActorId {
    gcore::exec::program_id().into()
}

/// Return the id of original user who initiated communication with blockchain,
/// during which, currently processing message was created.
///
/// # Examples
///
/// ```
/// use gstd::{exec, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let _user = exec::origin();
/// }
/// ```
pub fn origin() -> ActorId {
    gcore::exec::origin().into()
}
