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

use crate::{
    error::{Result, SyscallError},
    ActorId, MessageId, ReservationId,
};
use gear_core_errors::ExtError;
use gsys::{BlockNumberWithHash, LengthWithGas, LengthWithHash};

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
    let mut timestamp = 0u64;
    unsafe { gsys::gr_block_timestamp(&mut timestamp as *mut u64) };
    timestamp
}

/// Terminate the execution of a program. The program and all corresponding data
/// are removed from the storage. This is similar to
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
pub fn exit(inheritor_id: ActorId) -> ! {
    unsafe { gsys::gr_exit(inheritor_id.as_ptr()) }
}

/// Reserve gas for further usage.
///
/// # Examples
///
/// ```
/// use gcore::{exec, ReservationId};
///
/// static mut RESERVED: ReservationId = ReservationId::default();
///
/// unsafe extern "C" fn init() {
///     RESERVED = exec::reserve_gas(50_000_000, 7).unwrap();
/// }
///
/// unsafe extern "C" fn handle() {
///     exec::unreserve_gas(RESERVED);
/// }
/// ```
pub fn reserve_gas(amount: u64, duration: u32) -> Result<ReservationId> {
    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_reserve_gas(amount, duration, res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(res.hash.into())
}

/// Reserve gas for system usage.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// unsafe extern "C" fn handle() {
///     exec::system_reserve_gas(1_000_000).expect("enough gas");
///     exec::wait();
/// }
///
/// unsafe extern "C" fn handle_signal() {
///     // message removed from waitlist!
/// }
/// ```
pub fn system_reserve_gas(amount: u64) -> Result<()> {
    let mut len = 0u32;
    unsafe { gsys::gr_system_reserve_gas(amount, &mut len as *mut u32) };
    SyscallError(len).into_result()
}

/// Unreserve gas using reservation ID
///
/// Returns reserved gas amount.
///
/// # Examples
/// See [`reserve_gas`] example.
pub fn unreserve_gas(id: ReservationId) -> Result<u64> {
    let mut res: LengthWithGas = Default::default();

    unsafe { gsys::gr_unreserve_gas(id.as_ptr(), res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(res.gas)
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
    let mut gas = 0u64;
    unsafe { gsys::gr_gas_available(&mut gas as *mut u64) };
    gas
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
    unsafe { gsys::gr_leave() }
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
    let mut value = 0u128;
    unsafe { gsys::gr_value_available(&mut value as *mut u128) }
    value
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
/// This call delays message execution for maximal amount of blocks
/// that could be payed.
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
    unsafe { gsys::gr_wait() }
}

/// Same as [`wait`], but delays handling for given specific amount of blocks.
///
/// NOTE: It panics, if given duration couldn't be totally payed.
pub fn wait_for(duration: u32) -> ! {
    unsafe { gsys::gr_wait_for(duration) }
}

/// Same as [`wait`], but delays handling for maximal amount of blocks
/// that could be payed, that doesn't exceed given duration.
pub fn wait_up_to(duration: u32) -> ! {
    unsafe { gsys::gr_wait_up_to(duration) }
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
/// extern "C" fn handle() {
///     // ...
///     exec::wake(MessageId::default()).unwrap();
/// }
/// ```
pub fn wake(message_id: MessageId) -> Result<()> {
    wake_delayed(message_id, 0)
}

/// Same as [`wake`], but wakes delayed.
pub fn wake_delayed(message_id: MessageId, delay: u32) -> Result<()> {
    let mut len = 0u32;
    unsafe { gsys::gr_wake(message_id.as_ptr(), delay, &mut len as *mut u32) };
    SyscallError(len).into_result()
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
    let mut program_id = ActorId::default();
    unsafe { gsys::gr_program_id(program_id.as_mut_ptr()) }
    program_id
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
    let mut origin = ActorId::default();
    unsafe { gsys::gr_origin(origin.as_mut_ptr()) }
    origin
}

/// Get the random seed, along with the time in the past
/// since when it was determinable by chain observers.
/// The random seed is determined from the random seed of the block with the
/// message id as the subject.
///
/// `subject` is a context identifier and allows you to get a different results
/// within the execution. use it like `random(&b"my context"[..])`.
///
/// # Security
///
/// This MUST NOT be used for gambling, as it can be influenced by a
/// malicious validator in the short term. It MAY be used in many
/// cryptographic protocols, however, so long as one remembers that this
/// (like everything else on-chain) it is public. For example, it can be
/// used where a number is needed that cannot have been chosen by an
/// adversary, for purposes such as public-coin zero-knowledge proofs.
///
/// # Examples
///
/// ```
/// use gcore::exec;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let (seed, block_number) = exec::random(&b"my context"[..]);
/// }
/// ```
pub fn random(subject: &[u8]) -> Result<([u8; 32], u32)> {
    let mut res: BlockNumberWithHash = Default::default();

    let subject_len = subject
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe { gsys::gr_random(subject.as_ptr(), subject_len, res.as_mut_ptr()) };

    Ok((res.hash, res.bn))
}
