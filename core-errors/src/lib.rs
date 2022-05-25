// This file is part of Gear.
//
// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![no_std]

extern crate alloc;

use alloc::string::String;
use codec::{Decode, Encode};
use core::fmt;
use scale_info::TypeInfo;

pub trait CoreError: fmt::Display + fmt::Debug {}

/// Error using messages.
#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Decode,
    Encode,
    TypeInfo,
    derive_more::Display,
)]
pub enum MessageError {
    /// The error "Message limit exceeded" occurs when a program attempts to
    /// send more than the maximum amount of messages allowed within a single
    /// execution (current setting - 1024).
    #[display(fmt = "Message limit exceeded")]
    LimitExceeded,

    /// The error occurs in case of attempt to send more than one replies.
    #[display(fmt = "Duplicate reply message")]
    DuplicateReply,

    /// The error occurs in attempt to get the same message from the waitlist
    /// again (which is waked already).
    #[display(fmt = "Duplicate waking message")]
    DuplicateWaking,

    /// An attempt to commit or push a payload into an already formed message.
    #[display(fmt = "An attempt to commit or push a payload into an already formed message")]
    LateAccess,

    /// The error occurs in case of not valid identifier specified.
    #[display(fmt = "Message with given handle is not found")]
    OutOfBounds,

    /// The error occurs in attempt to initialize the same program twice within
    /// a single execution.
    #[display(fmt = "Duplicated program initialization message")]
    DuplicateInit,

    #[display(fmt = "User has no enough gas")]
    NotEnoughGas,

    #[display(
        fmt = "Value {} of the message in not in the range {{0}} ∪ [{}; +inf)",
        message_value,
        existential_deposit
    )]
    InsufficientValue {
        message_value: u128,
        existential_deposit: u128,
    },
    #[display(
        fmt = "{} is not enough value to send message with the value {}",
        value_left,
        message_value
    )]
    NotEnoughValue {
        message_value: u128,
        value_left: u128,
    },
}

/// Memory error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, derive_more::Display)]
pub enum MemoryError {
    /// The error occurs when a program tries to allocate more memory  than
    /// allowed.
    #[display(fmt = "Maximum possible memory has been allocated")]
    OutOfMemory,

    /// The error occurs in attempt to free-up a memory page from static area or
    /// outside additionally allocated for this program.
    #[display(fmt = "Page {} cannot be freed by the current program", _0)]
    InvalidFree(u32),

    /// The error occurs in attempt to access memory page outside pages area
    /// allocated for this program.
    #[display(fmt = "Access to the page not allocated to this program")]
    MemoryAccessError,

    /// WASM page does not contain all necesssary Gear pages
    #[display(fmt = "Page data has wrong size: {:#x}", _0)]
    InvalidPageDataSize(usize),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TerminationReason {
    Exit,
    Leave,
    Wait,
    GasAllowanceExceeded,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, derive_more::Display)]
pub enum ExtError {
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    Message(MessageError),
    #[display(fmt = "Not enough gas to continue execution")]
    GasLimitExceeded,
    #[display(fmt = "Too many gas refunded")]
    TooManyGasAdded,
    #[display(fmt = "Panic occurred: {}", _0)]
    PanicOccurred(String),
}

impl CoreError for ExtError {}
