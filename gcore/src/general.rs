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

//! Common structures used in Gear programs.
//!
//! This module contains definitions of common structures that are used to work
//! with Gear api.

/// Message handle.
///
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Message creation consists of the following parts - message
/// initialisation, filling the message with payload (can be gradual), message
/// sending.
///
/// Here are the functions that make up the parts of building and sending
/// messages: [`msg::send_init`](crate::msg::send_init), - message
/// initialization [`msg::send_push`](crate::msg::send_push), - add payload to a
/// message [`msg::send_commit`](crate::msg::send_commit) - send a message
///
/// In order to identify a message that is being built from parts of a program
/// you should use `MessageHandle` obtained via
/// [`msg::send_init`](crate::msg::send_init).
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init();
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle(pub u32);

/// Message identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Each message has its own unique 256-bit id. This id is represented
/// via the `MessageId` struct. Message identifier can be obtained for the
/// currently processed message using the [`msg::id`](crate::msg::id) function.
/// Also, each send and reply functions return a message identifier.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct MessageId(pub [u8; 32]);

impl MessageId {
    /// Create a new `MessageId` from the 32-byte slice `s`.
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to MessageId");
        }
        let mut id = Self([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    /// Get `MessageId` represented as a slice of `u8`.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

/// Program identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Source and target program as well as user are represented by
/// 256-bit identifier `ActorId` struct. The source `ActorId` for a message
/// being processed can be obtained using [`msg::source`](crate::msg::source)
/// function. Also, each send function has a target `ActorId` as one of the
/// arguments.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let program_id = msg::source();
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct ActorId(pub [u8; 32]);

impl From<u64> for ActorId {
    fn from(v: u64) -> Self {
        let mut id = ActorId([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
    }
}

impl ActorId {
    /// Create a new ActorId from 32-byte slice `s`.
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to ActorId");
        }
        let mut id = ActorId([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    /// Get `ActorId` represented as a slice of `u8`.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct CodeHash(pub [u8; 32]);

impl From<[u8; 32]> for CodeHash {
    fn from(v: [u8; 32]) -> Self {
        CodeHash(v)
    }
}

impl CodeHash {
    /// Create a new `H256` from 32-byte slice `s`.
    ///
    /// Panics if the supplied slice length is other than 32.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to H256");
        }
        let mut ret = CodeHash([0u8; 32]);
        ret.0[..].copy_from_slice(s);
        ret
    }

    /// Get `H256` represented as a slice of `u8`.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
}
