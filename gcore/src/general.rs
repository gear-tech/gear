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

//! Common structures used in Gear programs.
//!
//! This module contains definitions of common structures that are used to work
//! with Gear API.

use gsys::Hash;

#[cfg(not(feature = "stack_buffer"))]
use alloc::vec;

/// Message handle.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Message creation consists of the following parts: message
/// initialization, filling the message with payload (can be gradual), and
/// message sending.
///
/// Here are the functions that make up the parts of forming and sending
/// messages:
///
/// - [`msg::send_init`](crate::msg::send_init) initializes the message
/// - [`msg::send_push`](crate::msg::send_push) adds a payload to a
/// message
/// - [`msg::send_commit`](crate::msg::send_commit) sends a message
///
/// To identify a message that is being built from parts of a program, you
/// should use `MessageHandle` obtained via
/// [`msg::send_init`](crate::msg::send_init).
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello,").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit(msg_handle, msg::source(), 0).expect("Unable to send");
/// }
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle(pub(crate) u32);

/// Message identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Each message has its unique 256-bit identifier. The `MessageId`
/// struct represents this identifier. One can get the message identifier for
/// the currently processing message using the [`msg::id`](crate::msg::id)
/// function. Also, each send and reply functions return a message identifier.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct MessageId(pub [u8; 32]);

impl From<Hash> for MessageId {
    fn from(value: Hash) -> Self {
        MessageId(value)
    }
}

impl From<MessageId> for Hash {
    fn from(value: MessageId) -> Self {
        value.0
    }
}

impl MessageId {
    /// Create an empty `MessageId`.
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }
    /// Create a new `MessageId` from the 32-byte `slice`.
    ///
    /// # Panics
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(slice: &[u8]) -> Self {
        if slice.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to MessageId");
        }
        let mut id = Self([0u8; 32]);
        id.0.copy_from_slice(slice);
        id
    }

    /// Get `MessageId` represented as a slice of `u8`.
    pub const fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub(crate) const fn as_ptr(&self) -> *const [u8; 32] {
        self.0.as_ptr() as *const [u8; 32]
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut [u8; 32] {
        self.0.as_mut_ptr() as *mut [u8; 32]
    }
}

/// Program identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. `ActorId` struct represents the 256-bit identifier of the
/// source/target program or user. For example, the source `ActorId` for a
/// processing message can be obtained using the
/// [`msg::source`](crate::msg::source) function. Also, each send function has
/// an `ActorId` target as one of the arguments.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let sender = msg::source();
///     let user = exec::origin();
///     if sender == user {
///         msg::reply(b"Hello, user!", 0).expect("Unable to reply");
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct ActorId(pub [u8; 32]);

impl From<u64> for ActorId {
    fn from(value: u64) -> Self {
        let mut id = ActorId::zero();
        id.0[0..8].copy_from_slice(&value.to_le_bytes());
        id
    }
}

impl From<Hash> for ActorId {
    fn from(value: Hash) -> Self {
        ActorId(value)
    }
}

impl From<ActorId> for Hash {
    fn from(value: ActorId) -> Self {
        value.0
    }
}

impl ActorId {
    /// Create an empty `ActorId`.
    pub const fn zero() -> Self {
        Self([0; 32])
    }
    /// Create a new ActorId from 32-byte `slice`.
    ///
    /// # Panics
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(slice: &[u8]) -> Self {
        if slice.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to ActorId");
        }
        let mut id = ActorId::zero();
        id.0.copy_from_slice(slice);
        id
    }

    /// Get `ActorId` represented as a slice of `u8`.
    pub const fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub(crate) const fn as_ptr(&self) -> *const [u8; 32] {
        self.0.as_ptr() as *const [u8; 32]
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut [u8; 32] {
        self.0.as_mut_ptr() as *mut [u8; 32]
    }
}

/// Reservation identifier.
///
/// This identifier is used to get reserved gas or unreserve it.
///
/// See [`exec::reserve_gas`](crate::exec::reserve_gas).
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct ReservationId(pub [u8; 32]);

impl ReservationId {
    /// Create an empty `ReservationId`.
    pub const fn zero() -> Self {
        Self([0; 32])
    }
    /// Get `ReservationId` represented as a slice of `u8`.
    pub const fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub(crate) const fn as_ptr(&self) -> *const [u8; 32] {
        self.0.as_ptr() as *const [u8; 32]
    }
}

impl From<[u8; 32]> for ReservationId {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

// TODO: More info
/// Code identifier.
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct CodeId(pub [u8; 32]);

impl From<[u8; 32]> for CodeId {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl CodeId {
    /// Create a new `CodeId` from 32-byte `slice`.
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(slice: &[u8]) -> Self {
        if slice.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to CodeId");
        }
        let mut id = CodeId([0u8; 32]);
        id.0.copy_from_slice(slice);
        id
    }

    /// Get `CodeId` represented as a slice of `u8`.
    pub const fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// +_+_+
pub fn with_byte_buffer<F, R>(size: usize, f: F) -> R
where
    F: FnOnce(&mut [u8]) -> R,
{
    #[cfg(feature = "stack_buffer")]
    return crate::stack_buffer::with_byte_buffer(size, f);

    #[cfg(not(feature = "stack_buffer"))]
    {
        let mut buffer = vec![0u8; size];
        f(&mut buffer)
    }
}
