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

//! Common types used in GEAR programs.
//!
//! This module contains definition of common types that is used to work with
//! GEAR api,

/// Message handle.
///
/// GEAR allows users and programs to interact with other users and programs via
/// messages. There is a possibility to create and send messages in parts.
///
/// See [`msg::send_init`](crate::msg::send_init),
/// [`msg::send_push`](crate::msg::send_push), [`msg::send_commit`](crate::msg::send_commit) functions related to the building and sending messages in
/// parts. In order to identify message that is being built from parts program
/// must use `MessageHandle` obtained via
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageHandle(pub u32);

/// Message identifier.
///
/// GEAR allows users and programs to interact with other users and programs via
/// messages. Each message has its own unique id. This id is represented via
/// `MessageId` struct. Message identifier can be obtained for the current
/// message being processed using [`msg::id`](crate::msg::id) function. Also,
/// each send and reply function returns a message identifier.
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
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to MessageId");
        }
        let mut id = Self([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

/// 256-bit program identifier.
#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct ProgramId(pub [u8; 32]);

impl From<u64> for ProgramId {
    fn from(v: u64) -> Self {
        let mut id = ProgramId([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
    }
}

impl ProgramId {
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("The slice must contain 32 u8 to be casted to ProgramId");
        }
        let mut id = ProgramId([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}
