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

use core::fmt::Debug;
#[cfg(feature = "codec")]
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

pub use simple::*;

/// Error using messages.
#[allow(clippy::unnecessary_cast)]
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum MessageError {
    /// Message has bigger then allowed one message size
    #[display(fmt = "Max message size exceed")]
    MaxMessageSizeExceed = 0,

    /// Overflow in 'gr_read'
    #[display(fmt = "Length is overflowed ({at} + {len}) to read payload")]
    TooBigReadLen {
        /// Range starts at
        at: u32,
        /// Range length
        len: u32,
    } = 1,

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
    } = 2,

    /// The error "Message limit exceeded" occurs when a program attempts to
    /// send more than the maximum amount of messages allowed within a single
    /// execution (current setting - 1024).
    #[display(fmt = "Message limit exceeded")]
    OutgoingMessagesAmountLimitExceeded = 3,

    /// The error occurs in case of attempt to send more than one replies.
    #[display(fmt = "Duplicate reply message")]
    DuplicateReply = 4,

    /// The error occurs in attempt to get the same message from the waitlist
    /// again (which is waked already).
    #[display(fmt = "Duplicate waking message")]
    DuplicateWaking = 5,

    /// An attempt to commit or push a payload into an already formed message.
    #[display(fmt = "An attempt to commit or push a payload into an already formed message")]
    LateAccess = 6,

    /// The error occurs in case of not valid identifier specified.
    #[display(fmt = "Message with given handle is not found")]
    OutOfBounds = 7,

    /// The error occurs in attempt to initialize the same program twice within
    /// a single execution.
    #[display(fmt = "Duplicated program initialization message")]
    DuplicateInit = 8,

    /// An error occurs in attempt to send a message with more gas than available after previous message.
    #[display(fmt = "Not enough gas to send in message")]
    NotEnoughGas = 9,

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
    } = 10,

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
    } = 11,

    /// The error occurs when program's balance is less than value in message it tries to send.
    #[display(
        fmt = "Existing value {value_left} is not enough to send a message with value {message_value}"
    )]
    NotEnoughValue {
        /// Message's value.
        message_value: u128,
        /// Amount of available value.
        value_left: u128,
    } = 12,

    /// The error occurs when functions related to reply context, used without it.
    #[display(fmt = "Not running in reply context")]
    NoReplyContext = 13,

    /// The error occurs when functions related to signal context, used without it.
    #[display(fmt = "Not running in signal context")]
    NoSignalContext = 14,

    /// The error occurs when functions related to status code, used without required context.
    #[display(fmt = "No status code in reply/signal context")]
    NoStatusCodeContext = 15,

    /// An error occurs in attempt to charge gas for dispatch stash hold.
    #[display(fmt = "Not enough gas to hold dispatch message")]
    InsufficientGasForDelayedSending = 16,
}

/// Error using waiting syscalls.
#[allow(clippy::unnecessary_cast)]
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum WaitError {
    /// An error occurs in attempt to wait duration greater than could be payed.
    #[display(fmt = "Not enough gas to cover holding in waitlist")]
    NotEnoughGas = 0,
    /// An error occurs in attempt to wait duration greater than could be payed.
    #[display(fmt = "Provided incorrect argument for wait (zero case)")]
    InvalidArgument = 1,
}

/// Memory error.
#[allow(clippy::unnecessary_cast)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum MemoryError {
    /// The error occurs, when program tries to allocate in block-chain runtime more memory than allowed.
    #[display(fmt = "Trying to allocate more memory in block-chain runtime than allowed")]
    RuntimeAllocOutOfBounds = 0,
    /// The error occurs in attempt to access memory outside wasm program memory.
    #[display(fmt = "Trying to access memory outside wasm program memory")]
    AccessOutOfBounds = 1,
}

/// Reservation error.
#[allow(clippy::unnecessary_cast)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum ReservationError {
    /// An error occurs in attempt to unreserve gas with non-existing reservation ID.
    #[display(fmt = "Invalid reservation ID")]
    InvalidReservationId = 0,
    /// An error occurs in attempt to reserve more gas than available.
    #[display(fmt = "Insufficient gas for reservation")]
    InsufficientGasForReservation = 1,
    /// An error occurs in attempt to reserve more times than allowed.
    #[display(fmt = "Reservation limit has reached")]
    ReservationsLimitReached = 2,
    /// An error occurs in attempt to create reservation for 0 blocks.
    #[display(fmt = "Reservation duration cannot be zero")]
    ZeroReservationDuration = 3,
    /// An error occurs in attempt to reserve zero gas.
    #[display(fmt = "Reservation amount cannot be zero")]
    ZeroReservationAmount = 4,
    /// An error occurs in attempt to reserve gas less than mailbox threshold.
    #[display(fmt = "Reservation amount cannot be below mailbox threshold")]
    ReservationBelowMailboxThreshold = 5,
}

/// Execution error.
#[allow(clippy::unnecessary_cast)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum ExecutionError {
    /// An error occurs in attempt to parse invalid string in `gr_debug` sys-call.
    #[display(fmt = "Invalid debug string passed in `gr_debug` sys-call")]
    InvalidDebugString = 0,
}

/// An error occurred in API.
#[allow(clippy::unnecessary_cast)]
#[derive(
    Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, derive_more::Display, derive_more::From,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
#[non_exhaustive]
#[repr(u8)]
pub enum ExtError {
    /// We got some error but don't know which exactly because of disabled gcore's `codec` feature
    #[cfg(not(feature = "codec"))]
    #[display(fmt = "Some error")]
    Some = 0,

    // TODO: consider to create more complex one.
    /// Syscall usage error.
    #[display(fmt = "Syscall usage error")]
    SyscallUsage = 1,

    /// Memory error.
    #[display(fmt = "Memory error: {_0}")]
    Memory(MemoryError) = 2,

    /// Message error.
    #[display(fmt = "Message error: {_0}")]
    Message(MessageError) = 3,

    /// Waiting error.
    #[display(fmt = "Waiting error: {_0}")]
    Wait(WaitError) = 4,

    /// Reservation error.
    #[display(fmt = "Reservation error: {_0}")]
    Reservation(ReservationError) = 5,

    /// Execution error.
    #[display(fmt = "Execution error: {_0}")]
    Execution(ExecutionError) = 6,
}

#[cfg(feature = "codec")]
impl ExtError {
    /// Size of error encoded in SCALE codec
    pub fn encoded_size(&self) -> usize {
        Encode::encoded_size(self)
    }
}
