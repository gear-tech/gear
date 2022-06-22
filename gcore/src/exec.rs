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
//! Provides API for low-level async implementation.

use crate::{ActorId, MessageId};

mod sys {
    extern "C" {
        pub fn gr_block_height() -> u32;
        pub fn gr_block_timestamp() -> u64;
        pub fn gr_exit(value_dest_ptr: *const u8) -> !;
        pub fn gr_gas_available() -> u64;
        pub fn gr_program_id(val: *mut u8);
        pub fn gr_origin(origin_ptr: *mut u8);
        pub fn gr_leave() -> !;
        pub fn gr_value_available(val: *mut u8);
        pub fn gr_wait() -> !;
        pub fn gr_wake(waker_id_ptr: *const u8);
    }
}

/// Get the current block height.
///
/// The block height serves to identify a particular block.
/// This information can be used to enable many scenarios like restricting or
/// allowing of some functions until certain block height is reached.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// // Send a reply after the block height reaches the number 1000
/// unsafe extern "C" fn handle() {
///     if exec::block_height() >= 1000 {
///         msg::reply(b"Block #1000 reached", 0).unwrap();
///     }
/// }
/// ```
pub fn block_height() -> u32 {
    unsafe { sys::gr_block_height() }
}

/// Get the current block timestamp.
///
/// The timestamp is the number of milliseconds elapsed since the Unix epoch.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// // Send a reply after the block timestamp reaches the February 22, 2022
/// unsafe extern "C" fn handle() {
///     if exec::block_timestamp() >= 1645488000000 {
///         msg::reply(b"The current block is generated after February 22, 2022", 0).unwrap();
///     }
/// }
/// ```
pub fn block_timestamp() -> u64 {
    unsafe { sys::gr_block_timestamp() }
}

/// Terminate the execution of a program. The program and all corresponding data
/// are removed from the storage. This is similiar to
/// `std::process::exit`. `value_destination` specifies the address where all
/// available to the program value should be transferred to.
/// Maybe called in `init` method as well.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::exit(msg::source());
/// }
/// ```
pub fn exit(value_destination: ActorId) -> ! {
    unsafe { sys::gr_exit(value_destination.as_slice().as_ptr()) }
}

/// Get the current value of the gas available for execution.
///
/// Each message processing consumes gas, both on instructions execution and
/// memory allocations. This function returns a value of the gas available for
/// spending during current execution. Its use may help to avoid unexpected
/// behaviors during the smart-contract execution in case of not enough gas
/// available.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// // Perform work while `gas_available` is more than 1000
/// unsafe extern "C" fn handle() {
///     while exec::gas_available() > 1000 {
///         // ...
///     }
/// }
/// ```
pub fn gas_available() -> u64 {
    unsafe { sys::gr_gas_available() }
}

/// Terminate the current message handling.
///
/// For cases when the message handling needs to be terminated with state
/// saving.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::leave();
/// }
/// ```
pub fn leave() -> ! {
    unsafe { sys::gr_leave() }
}

/// Get the total available value amount.
///
/// Note that value received with currently processing message
/// is already included in this balance.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// // Get self value balance in program
/// unsafe extern "C" fn handle() {
///     let _my_balance = exec::value_available();
/// }
/// ```
pub fn value_available() -> u128 {
    let mut value_data = [0u8; 16];
    unsafe {
        sys::gr_value_available(value_data.as_mut_ptr());
    }
    u128::from_le_bytes(value_data)
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
/// use gcore::exec;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::wait();
/// }
/// ```
pub fn wait() -> ! {
    unsafe { sys::gr_wait() }
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
/// use gcore::{exec, MessageId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     exec::wake(MessageId::default());
/// }
/// ```
pub fn wake(waker_id: MessageId) {
    unsafe {
        sys::gr_wake(waker_id.as_slice().as_ptr());
    }
}

/// Return ID of the current program.
///
/// # Examples
///
/// ```
/// use gcore::{exec, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let me = exec::program_id();
/// }
/// ```
pub fn program_id() -> ActorId {
    let mut actor_id = ActorId::default();
    unsafe { sys::gr_program_id(actor_id.as_mut_slice().as_mut_ptr()) }
    actor_id
}

/// Return the id of original user who initiated communication with blockchain,
/// during which, currently processing message was created.
///
/// # Examples
///
/// ```
/// use gcore::{exec, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let _user = exec::origin();
/// }
/// ```
pub fn origin() -> ActorId {
    let mut actor_id = ActorId::default();
    unsafe { sys::gr_origin(actor_id.as_mut_slice().as_mut_ptr()) };
    actor_id
}
