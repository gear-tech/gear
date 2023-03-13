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
//! This module also provides API for low-level async implementation.

use crate::{
    errors::{Result, SyscallError},
    ActorId, MessageId, ReservationId,
};
use gsys::{BlockNumberWithHash, LengthWithGas, LengthWithHash};

/// Get the current block height.
///
/// The block height serves to identify a particular block.
/// This information can be used to enable many scenarios, like restricting or
/// allowing some functions until a particular block height is reached.
///
/// # Examples
///
/// Send a reply after the block height reaches the number 1000:
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     if exec::block_height() >= 1000 {
///         msg::reply(b"Block #1000 reached", 0).unwrap();
///     }
/// }
/// ```
pub fn block_height() -> u32 {
    let mut bn = 0u32;
    unsafe { gsys::gr_block_height(&mut bn as *mut u32) };
    bn
}

/// Get the current block timestamp.
///
/// The timestamp is the number of milliseconds elapsed since the Unix epoch.
///
/// # Examples
///
/// Send a reply after the block timestamp reaches February 22, 2022:
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     if exec::block_timestamp() >= 1645488000000 {
///         msg::reply(b"The current block is generated after February 22, 2022", 0)
///             .expect("Unable to reply");
///     }
/// }
/// ```
pub fn block_timestamp() -> u64 {
    let mut timestamp = 0u64;
    unsafe { gsys::gr_block_timestamp(&mut timestamp as *mut u64) };
    timestamp
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
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // ...
///     exec::exit(msg::source());
/// }
/// ```
pub fn exit(inheritor_id: ActorId) -> ! {
    unsafe { gsys::gr_exit(inheritor_id.as_ptr()) }
}

/// Reserve the `amount` of gas for further usage.
///
/// `duration` is the block count within which the reserve must be used.
///
/// This function returns [`ReservationId`], which one can use for gas
/// unreserving.
///
/// # Examples
///
/// Reserve 50 million of gas for seven blocks:
///
/// ```
/// use gcore::{exec, ReservationId};
///
/// static mut RESERVED: ReservationId = ReservationId::zero();
///
/// #[no_mangle]
/// extern "C" fn init() {
///     unsafe { RESERVED = exec::reserve_gas(50_000_000, 7).unwrap() };
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     exec::unreserve_gas(unsafe { RESERVED });
/// }
/// ```
///
/// # See also
///
/// - [`unreserve_gas`] function unreserves gas identified by [`ReservationId`].
/// - [`system_reserve_gas`] function reserves gas for system usage.
pub fn reserve_gas(amount: u64, duration: u32) -> Result<ReservationId> {
    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_reserve_gas(amount, duration, res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(res.hash.into())
}

/// Reserve the `amount` of gas for system usage.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     exec::system_reserve_gas(1_000_000).expect("Unable to reserve");
///     exec::wait();
/// }
///
/// #[no_mangle]
/// extern "C" fn handle_signal() {
///     // Message removed from waitlist!
/// }
/// ```
///
/// # See also
///
/// - [`reserve_gas`] function reserves gas for further usage.
pub fn system_reserve_gas(amount: u64) -> Result<()> {
    let mut len = 0u32;
    unsafe { gsys::gr_system_reserve_gas(amount, &mut len as *mut u32) };
    SyscallError(len).into_result()
}

/// Unreserve gas identified by [`ReservationId`].
///
/// If successful, it returns the reserved amount of gas.
///
/// # Examples
///
/// See [`reserve_gas`] examples.
///
/// # See also
///
/// - [`reserve_gas`] function reserves gas for further usage.
pub fn unreserve_gas(id: ReservationId) -> Result<u64> {
    let mut res: LengthWithGas = Default::default();

    unsafe { gsys::gr_unreserve_gas(id.as_ptr(), res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(res.gas)
}

/// Get the current amount of gas available for execution.
///
/// Each message processing consumes gas on instructions execution and memory
/// allocations. This function returns a value of the gas available for spending
/// during the current execution. Its use may help avoid unexpected behaviors
/// during the smart-contract execution in case insufficient gas is available.
///
/// # Examples
///
/// Do the job while the amount of available gas is more than 1000:
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     while exec::gas_available() > 1000 {
///         // ...
///     }
/// }
/// ```
pub fn gas_available() -> u64 {
    let mut gas = 0u64;
    unsafe { gsys::gr_gas_available(&mut gas as *mut u64) };
    gas
}

/// Break the current execution.
///
/// Use this function to break the current message processing and save the
/// state.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     if exec::gas_available() < 1_000_000 {
///         exec::leave();
///     }
/// }
/// ```
pub fn leave() -> ! {
    unsafe { gsys::gr_leave() }
}

/// Get the total available value amount.
///
/// Note that this balance already includes the value received with the current
/// message.
///
/// # Examples
///
/// Get self's value balance in the program:
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let my_balance = exec::value_available();
/// }
/// ```
pub fn value_available() -> u128 {
    let mut value = 0u128;
    unsafe { gsys::gr_value_available(&mut value as *mut u128) }
    value
}

/// Pause the current message handling.
///
/// If the message handling needs to be paused, e.g., to wait for another
/// execution to finish, one should use this function. [`wait`] completes the
/// current message handles execution with a special result and puts this
/// message into the *waiting queue* to be awakened using the correspondent
/// [`wake`] function later. All gas that hasn't yet been spent is attributed to
/// the message in the *waiting queue*.
///
/// This call delays message execution for a maximum amount of blocks that could
/// be paid.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // ...
///     exec::wait();
/// }
/// ```
pub fn wait() -> ! {
    unsafe { gsys::gr_wait() }
}

/// Same as [`wait`], but delays handling for a specific number of blocks.
///
/// # Panics
///
/// Panics if it is impossible to pay the given `duration`.
pub fn wait_for(duration: u32) -> ! {
    unsafe { gsys::gr_wait_for(duration) }
}

/// Same as [`wait`], but delays handling for the maximum number of blocks that
/// can be paid for and doesn't exceed the given `duration`.
pub fn wait_up_to(duration: u32) -> ! {
    unsafe { gsys::gr_wait_up_to(duration) }
}

/// Resume previously paused message handling.
///
/// Suppose a message has been paused using the [`wait`] function. In that case,
/// it is possible to continue its execution by calling this function.
///
/// `message_id` specifies a particular message to be taken out of the *waiting
/// queue* and put into the *processing queue*.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg, MessageId};
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
    let mut len = 0u32;
    unsafe { gsys::gr_wake(message_id.as_ptr(), delay, &mut len as *mut u32) };
    SyscallError(len).into_result()
}

/// Return the identifier of the current program.
///
/// # Examples
///
/// ```
/// use gcore::{exec, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let whoami = exec::program_id();
/// }
/// ```
pub fn program_id() -> ActorId {
    let mut program_id = ActorId::default();
    unsafe { gsys::gr_program_id(program_id.as_mut_ptr()) }
    program_id
}

/// Return the identifier of the original user who initiated communication with
/// the blockchain, during which the currently processing message was created.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let user = exec::origin();
/// }
/// ```
pub fn origin() -> ActorId {
    let mut origin = ActorId::default();
    unsafe { gsys::gr_origin(origin.as_mut_ptr()) }
    origin
}

/// Get the random seed, along with the block number from which it is
/// determinable by chain observers.
///
/// `subject` is a context identifier that allows you to get different results
/// within the execution.
///
/// # Security
///
/// This **must NOT** be used for gambling, as it can be influenced by a
/// malicious validator in the short term. It **MAY** be used in many
/// cryptographic protocols, however, so long as one remembers that this (like
/// everything else on-chain) is public. For example, it can be used when a
/// number is needed that an adversary cannot choose for such purposes as
/// public-coin zero-knowledge proofs.
///
/// # Examples
///
/// ```
/// use core::array;
/// use gcore::exec;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let subject: [u8; 32] = array::from_fn(|i| i as u8 + 1);
///     let (seed, block_number) = exec::random(subject).expect("Error in random");
/// }
/// ```
pub fn random(subject: [u8; 32]) -> Result<([u8; 32], u32)> {
    let mut res: BlockNumberWithHash = Default::default();

    unsafe { gsys::gr_random(subject.as_ptr(), res.as_mut_ptr()) };

    Ok((res.hash, res.bn))
}
