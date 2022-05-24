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

pub trait CoreError: fmt::Display + fmt::Debug {
    fn from_termination_reason(reason: TerminationReason) -> Self;

    fn as_termination_reason(&self) -> Option<TerminationReason>;
}

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
    /// Message limit exceeded.
    #[display(fmt = "Message limit exceeded")]
    LimitExceeded,
    /// Duplicate reply message.
    #[display(fmt = "Duplicate reply message")]
    DuplicateReply,
    /// Duplicate waking message.
    #[display(fmt = "Duplicate waking message")]
    DuplicateWaking,
    /// An attempt to commit or to push a payload into an already formed message.
    #[display(fmt = "An attempt to commit or to push a payload into an already formed message")]
    LateAccess,
    /// No message found with given handle, or handle exceeds the maximum messages amount.
    #[display(
        fmt = "No message found with given handle, or handle exceeds the maximum messages amount"
    )]
    OutOfBounds,
    /// Duplicate init message
    #[display(fmt = "Duplicate init message")]
    DuplicateInit,
    #[display(fmt = "User has no enough gas")]
    NotEnoughGas,
}

/// Memory error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, derive_more::Display)]
pub enum MemoryError {
    /// Memory is over.
    ///
    /// All pages were previously allocated and there is nothing can be done.
    #[display(fmt = "Memory is over")]
    OutOfMemory,

    /// Specified page cannot be freed by the current program.
    ///
    /// It was allocated by another program.
    #[display(fmt = "Page {} cannot be freed by the current program", _0)]
    InvalidFree(u32),

    /// Out of bounds memory access
    #[display(fmt = "Out of bounds memory access")]
    MemoryAccessError,

    /// There is wasm page, which has not all gear pages in the begin
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
    #[display(fmt = "Allocation error: {}", _0)]
    Alloc(MemoryError),
    #[display(fmt = "Free error: {}", _0)]
    Free(MemoryError),
    #[display(fmt = "Gas limit exceeded during code execution")]
    GasLimitExceeded,
    #[display(fmt = "Too many gas added")]
    TooManyGasAdded,
    #[display(fmt = "Terminated: {:?}", _0)]
    TerminationReason(TerminationReason),
    #[display(fmt = "Failed to wake the message: {}", _0)]
    Wake(MessageError),
    #[display(fmt = "{}", _0)]
    InitMessageNotDuplicated(MessageError),
    #[display(fmt = "Panic occurred: {}", _0)]
    PanicOccurred(String),
    #[display(
        fmt = "Value {} of the message in not in the range {{0}} ∪ [{}; +inf)",
        message_value,
        existential_deposit
    )]
    InsufficientMessageValue {
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
    #[display(fmt = "{}", _0)]
    Message(MessageError),
}

impl CoreError for ExtError {
    fn from_termination_reason(reason: TerminationReason) -> Self {
        Self::TerminationReason(reason)
    }

    fn as_termination_reason(&self) -> Option<TerminationReason> {
        match self {
            ExtError::TerminationReason(reason) => Some(*reason),
            _ => None,
        }
    }
}
