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

//! Gear core errors.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

mod simple;

#[cfg(feature = "codec")]
use codec::{Decode, Encode};
use core::fmt::{Debug, Display};
#[cfg(feature = "codec")]
use scale_info::TypeInfo;

pub use simple::*;

/// Core error.
pub trait CoreError: Debug + Display + Sized {}

/// Error using messages.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum MessageError {
    /// Message has bigger then allowed one message size
    #[display(fmt = "Max message size exceed")]
    MaxMessageSizeExceed,

    /// Overflow in 'gr_read'
    #[display(fmt = "Length is overflowed ({at} + {len}) to read payload")]
    TooBigReadLen {
        /// Range starts at
        at: u32,
        /// Range length
        len: u32,
    },

    /// Cannot take data in payload range
    #[display(
        fmt = "Cannot take data in payload range [{start}; {end}) from message with size {msg_len}"
    )]
    ReadWrongRange {
        /// Range starts at
        start: u32,
        /// Range ends at
        end: u32,
        /// Message length
        msg_len: u32,
    },

    /// The error "Message limit exceeded" occurs when a program attempts to
    /// send more than the maximum amount of messages allowed within a single
    /// execution (current setting - 1024).
    #[display(fmt = "Message limit exceeded")]
    OutgoingMessagesAmountLimitExceeded,

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

    /// An error occurs in attempt to send a message with more gas than available after previous message.
    #[display(fmt = "Not enough gas to send in message")]
    NotEnoughGas,

    /// Everything less than existential deposit but greater than 0 is not considered as available balance and not saved in DB.
    /// Value between 0 and existential deposit cannot be sent in message.
    #[display(
        fmt = "In case of non-zero message value {message_value}, it must be greater than existential deposit {existential_deposit}"
    )]
    InsufficientValue {
        /// Message's value.
        message_value: u128,
        /// Minimal amount of funds on a balance that can be considered and added in DB.
        existential_deposit: u128,
    },

    /// Everything less than mailbox threshold but greater than 0 is not considered as available gas limit and
    /// not inserted in mailbox.
    ///
    /// Gas limit between 0 and mailbox threshold cannot be inserted in mailbox.
    #[display(
        fmt = "In case of non-zero message gas limit {message_gas_limit}, it must be greater than mailbox threshold {mailbox_threshold}"
    )]
    InsufficientGasLimit {
        /// Message's gas limit.
        message_gas_limit: u64,
        /// Minimal amount of gas limit on a message that can be inserted in mailbox.
        mailbox_threshold: u64,
    },

    /// The error occurs when program's balance is less than value in message it tries to send.
    #[display(
        fmt = "Existing value {value_left} is not enough to send a message with value {message_value}"
    )]
    NotEnoughValue {
        /// Message's value.
        message_value: u128,
        /// Amount of available value.
        value_left: u128,
    },

    /// The error occurs when functions related to reply context, used without it.
    #[display(fmt = "Not running in reply context")]
    NoReplyContext,

    /// The error occurs when functions related to signal context, used without it.
    #[display(fmt = "Not running in signal context")]
    NoSignalContext,

    /// The error occurs when functions related to status code, used without required context.
    #[display(fmt = "No status code in reply/signal context")]
    NoStatusCodeContext,
}

/// Error using waiting syscalls.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum WaitError {
    /// An error occurs in attempt to wait duration greater than could be payed.
    #[display(fmt = "Not enough gas to cover holding in waitlist")]
    NotEnoughGas,
    /// An error occurs in attempt to wait duration greater than could be payed.
    #[display(fmt = "Provided incorrect argument for wait (zero case)")]
    InvalidArgument,
}

/// Memory error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum MemoryError {
    /// The error occurs when a program tries to allocate more memory than
    /// allowed.
    #[display(fmt = "Trying to allocate more wasm program memory than allowed")]
    ProgramAllocOutOfBounds,

    /// The error occurs, when program tries to allocate in block-chain runtime more memory than allowed.
    #[display(fmt = "Trying to allocate more memory in block-chain runtime than allowed")]
    RuntimeAllocOutOfBounds,

    /// The error occurs in attempt to free-up a memory page from static area or
    /// outside additionally allocated for this program.
    #[display(fmt = "Page {_0} cannot be freed by the current program")]
    InvalidFree(u32),

    /// The error occurs in attempt to access memory outside wasm program memory.
    #[display(fmt = "Trying to access memory outside wasm program memory")]
    AccessOutOfBounds,
}

/// Reservation error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum ReservationError {
    /// An error occurs in attempt to unreserve gas with non-existing reservation ID.
    #[display(fmt = "Invalid reservation ID")]
    InvalidReservationId,
    /// An error occurs in attempt to reserve more gas than available.
    #[display(fmt = "Insufficient gas for reservation")]
    InsufficientGasForReservation,
    /// An error occurs in attempt to reserve more times than allowed.
    #[display(fmt = "Reservation limit has reached")]
    ReservationsLimitReached,
    /// An error occurs in attempt to create reservation for 0 blocks.
    #[display(fmt = "Reservation duration cannot be zero")]
    ZeroReservationDuration,
    /// An error occurs in attempt to reserve zero gas.
    #[display(fmt = "Reservation amount cannot be zero")]
    ZeroReservationAmount,
}

/// Execution error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum ExecutionError {
    /// An error occurs in attempt to parse invalid string in `gr_debug` sys-call.
    #[display(fmt = "Invalid debug string passed in `gr_debug` sys-call")]
    InvalidDebugString,
}

/// An error occurred in API.
#[derive(
    Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, derive_more::Display, derive_more::From,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[non_exhaustive]
pub enum ExtError {
    /// We got some error but don't know which exactly because of disabled gcore's `codec` feature
    #[cfg(not(feature = "codec"))]
    #[display(fmt = "Some error")]
    Some,

    /// Decode error.
    ///
    /// Supposed to be unreachable.
    #[cfg(feature = "codec")]
    #[display(fmt = "`ExtError` decoding error")]
    Decode,

    // TODO: consider to create more complex one.
    /// Syscall usage error.
    #[display(fmt = "Syscall usage error")]
    SyscallUsage,

    /// Memory error.
    #[display(fmt = "Memory error: {_0}")]
    Memory(MemoryError),

    /// Message error.
    #[display(fmt = "Message error: {_0}")]
    Message(MessageError),

    /// Waiting error.
    #[display(fmt = "Waiting error: {_0}")]
    Wait(WaitError),

    /// Reservation error.
    #[display(fmt = "Reservation error: {_0}")]
    Reservation(ReservationError),

    /// Execution error.
    #[display(fmt = "Execution error: {_0}")]
    Execution(ExecutionError),
}

#[cfg(feature = "codec")]
impl ExtError {
    /// Size of error encoded in SCALE codec
    pub fn encoded_size(&self) -> usize {
        Encode::encoded_size(self)
    }
}
