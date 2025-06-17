// This file is part of Gear.
//
// Copyright (C) 2022-2025 Gear Technologies Inc.
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
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

extern crate alloc;

mod simple;

use core::fmt::Debug;
use enum_iterator::Sequence;
#[cfg(feature = "codec")]
use {
    alloc::vec::Vec,
    scale_info::scale::{Decode, Encode, Error, Input},
};

pub use simple::*;

/// Execution error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Sequence, thiserror::Error)]
#[non_exhaustive]
#[repr(u32)]
pub enum ExecutionError {
    /// An error occurs in attempt to charge more gas than available for operation.
    #[error("Not enough gas for operation")]
    NotEnoughGas = 100,

    /// The error occurs when balance is less than required by operation.
    #[error("Not enough value for operation")]
    NotEnoughValue = 101,

    /// Overflow in 'gr_read'
    #[error("Length is overflowed to read payload")]
    TooBigReadLen = 103,

    /// Cannot take data in payload range
    #[error("Cannot take data in payload range from message with size")]
    ReadWrongRange = 104,

    /// The error occurs when functions related to reply context, used without it.
    #[error("Not running in reply context")]
    NoReplyContext = 105,

    /// The error occurs when functions related to signal context, used without it.
    #[error("Not running in signal context")]
    NoSignalContext = 106,

    /// The error occurs when functions related to status code, used without required context.
    #[error("No status code in reply/signal context")]
    NoStatusCodeContext = 107,

    /// An error occurs in attempt to send or push reply while reply function is banned.
    #[error("Reply sending is only allowed in `init` and `handle` functions")]
    IncorrectEntryForReply = 108,
}

/// Memory error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, Sequence, thiserror::Error)]
#[non_exhaustive]
#[repr(u32)]
pub enum MemoryError {
    /// The error occurs, when program tries to allocate in block-chain runtime more memory than allowed.
    #[error("Trying to allocate more memory in block-chain runtime than allowed")]
    RuntimeAllocOutOfBounds = 200,
    /// The error occurs in attempt to access memory outside wasm program memory.
    #[error("Trying to access memory outside wasm program memory")]
    AccessOutOfBounds = 201,
}

/// Error using messages.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Sequence, thiserror::Error)]
#[non_exhaustive]
#[repr(u32)]
pub enum MessageError {
    /// Message has bigger then allowed one message size
    #[error("Max message size exceed")]
    MaxMessageSizeExceed = 300,

    /// The error "Message limit exceeded" occurs when a program attempts to
    /// send more than the maximum amount of messages allowed within a single
    /// execution (current setting - 1024).
    #[error("Message limit exceeded")]
    OutgoingMessagesAmountLimitExceeded = 301,

    /// The error occurs in case of attempt to send more than one replies.
    #[error("Duplicate reply message")]
    DuplicateReply = 302,

    /// The error occurs in attempt to get the same message from the waitlist
    /// again (which is waked already).
    #[error("Duplicate waking message")]
    DuplicateWaking = 303,

    /// An attempt to commit or push a payload into an already formed message.
    #[error("An attempt to commit or push a payload into an already formed message")]
    LateAccess = 304,

    /// The error occurs in case of not valid identifier specified.
    #[error("Message with given handle is not found")]
    OutOfBounds = 305,

    /// The error occurs in attempt to initialize the same program twice within
    /// a single execution.
    #[error("Duplicated program initialization message")]
    DuplicateInit = 306,

    /// Everything less than existential deposit but greater than 0 is not considered as available balance and not saved in DB.
    /// Value between 0 and existential deposit cannot be sent in message.
    #[error("In case of non-zero message value must be greater than existential deposit")]
    InsufficientValue = 307,

    /// Everything less than mailbox threshold but greater than 0 is not considered as available gas limit and
    /// not inserted in mailbox.
    ///
    /// Gas limit between 0 and mailbox threshold cannot be inserted in mailbox.
    #[error("In case of non-zero message gas limit must be greater than mailbox threshold")]
    InsufficientGasLimit = 308,

    /// The error occurs when program tries to create reply deposit for message
    /// that already been created within the execution.
    #[error("Reply deposit already exists for given message")]
    DuplicateReplyDeposit = 309,

    /// The error occurs when program tries to create reply deposit for message
    /// that wasn't sent within the execution or for reply.
    #[error(
        "Reply deposit could be only created for init or handle message sent within the execution"
    )]
    IncorrectMessageForReplyDeposit = 310,

    /// The error occurs when program tries to send messages
    /// with total size bigger than allowed.
    #[error("Outgoing messages bytes limit exceeded")]
    OutgoingMessagesBytesLimitExceeded = 311,

    /// The error occurs when a wrong offset of the input buffer (currently executing message payload)
    /// is provided.
    #[error("Offset value for the input payload is out of it's size bounds")]
    OutOfBoundsInputSliceOffset = 312,

    /// The error occurs when a too big length value to form a slice (range) of the input buffer
    /// (currently executing message payload) is provided.
    #[error("Too big length value is set to form a slice (range) of the input buffer")]
    OutOfBoundsInputSliceLength = 313,

    // TODO: remove after delay refactoring is done
    /// An error occurs in attempt to charge gas for dispatch stash hold.
    #[error("Not enough gas to hold dispatch message")]
    InsufficientGasForDelayedSending = 399,
}

/// Reservation error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, Sequence, thiserror::Error)]
#[non_exhaustive]
#[repr(u32)]
// TODO: refactor after multiple reservations are done
pub enum ReservationError {
    /// An error occurs in attempt to unreserve gas with non-existing reservation ID.
    #[error("Invalid reservation ID")]
    InvalidReservationId = 500,
    /// An error occurs in attempt to reserve more times than allowed.
    #[error("Reservation limit has reached")]
    ReservationsLimitReached = 501,
    /// An error occurs in attempt to create reservation for 0 blocks.
    #[error("Reservation duration cannot be zero")]
    ZeroReservationDuration = 502,
    /// An error occurs in attempt to reserve zero gas.
    #[error("Reservation amount cannot be zero")]
    ZeroReservationAmount = 503,
    /// An error occurs in attempt to reserve gas less than mailbox threshold.
    #[error("Reservation amount cannot be below mailbox threshold")]
    ReservationBelowMailboxThreshold = 504,
}

/// An error occurred in API.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord, Sequence, thiserror::Error)]
#[non_exhaustive]
pub enum ExtError {
    /// Execution error.
    #[error("Execution error: {0}")]
    Execution(#[from] ExecutionError),

    /// Memory error.
    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),

    /// Message error.
    #[error("Message error: {0}")]
    Message(#[from] MessageError),

    /// Reservation error.
    #[error("Reservation error: {0}")]
    Reservation(#[from] ReservationError),

    /// There is a new error variant old program don't support.
    #[error("Unsupported error")]
    Unsupported,
}

impl ExtError {
    /// Convert error into code.
    pub fn to_u32(self) -> u32 {
        match self {
            ExtError::Execution(err) => err as u32,
            ExtError::Memory(err) => err as u32,
            ExtError::Message(err) => err as u32,
            ExtError::Reservation(err) => err as u32,
            ExtError::Unsupported => u32::MAX,
        }
    }

    /// Convert code into error.
    pub fn from_u32(code: u32) -> Option<Self> {
        match code {
            100 => Some(ExecutionError::NotEnoughGas.into()),
            101 => Some(ExecutionError::NotEnoughValue.into()),
            103 => Some(ExecutionError::TooBigReadLen.into()),
            104 => Some(ExecutionError::ReadWrongRange.into()),
            105 => Some(ExecutionError::NoReplyContext.into()),
            106 => Some(ExecutionError::NoSignalContext.into()),
            107 => Some(ExecutionError::NoStatusCodeContext.into()),
            108 => Some(ExecutionError::IncorrectEntryForReply.into()),
            //
            200 => Some(MemoryError::RuntimeAllocOutOfBounds.into()),
            201 => Some(MemoryError::AccessOutOfBounds.into()),
            //
            300 => Some(MessageError::MaxMessageSizeExceed.into()),
            301 => Some(MessageError::OutgoingMessagesAmountLimitExceeded.into()),
            302 => Some(MessageError::DuplicateReply.into()),
            303 => Some(MessageError::DuplicateWaking.into()),
            304 => Some(MessageError::LateAccess.into()),
            305 => Some(MessageError::OutOfBounds.into()),
            306 => Some(MessageError::DuplicateInit.into()),
            307 => Some(MessageError::InsufficientValue.into()),
            308 => Some(MessageError::InsufficientGasLimit.into()),
            309 => Some(MessageError::DuplicateReplyDeposit.into()),
            310 => Some(MessageError::IncorrectMessageForReplyDeposit.into()),
            311 => Some(MessageError::OutgoingMessagesBytesLimitExceeded.into()),
            312 => Some(MessageError::OutOfBoundsInputSliceOffset.into()),
            313 => Some(MessageError::OutOfBoundsInputSliceLength.into()),
            399 => Some(MessageError::InsufficientGasForDelayedSending.into()),
            //
            500 => Some(ReservationError::InvalidReservationId.into()),
            501 => Some(ReservationError::ReservationsLimitReached.into()),
            502 => Some(ReservationError::ZeroReservationDuration.into()),
            503 => Some(ReservationError::ZeroReservationAmount.into()),
            504 => Some(ReservationError::ReservationBelowMailboxThreshold.into()),
            //
            0xffff /* SyscallUsage */ |
            600 /* ProgramRent(ProgramRentError::MaximumBlockCountPaid) */ |
            u32::MAX => Some(ExtError::Unsupported),
            _ => None,
        }
    }
}

#[cfg(feature = "codec")]
impl Encode for ExtError {
    fn encode(&self) -> Vec<u8> {
        ExtError::to_u32(*self).to_le_bytes().to_vec()
    }
}

#[cfg(feature = "codec")]
impl Decode for ExtError {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut code = [0; 4];
        input.read(&mut code)?;
        let err =
            ExtError::from_u32(u32::from_le_bytes(code)).ok_or("Failed to decode error code")?;
        Ok(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    #[test]
    fn error_codes_are_unique() {
        let mut codes = BTreeMap::new();

        for err in enum_iterator::all::<ExtError>() {
            let code = err.to_u32();
            if let Some(same_code_err) = codes.insert(code, err) {
                panic!("{:?} has same code {:?} as {:?}", same_code_err, code, err);
            }
        }
    }

    #[test]
    fn encode_decode() {
        for err in enum_iterator::all::<ExtError>() {
            let code = err.to_u32();
            let decoded = ExtError::from_u32(code)
                .unwrap_or_else(|| unreachable!("failed to decode error code: {}", code));
            assert_eq!(err, decoded);
        }
    }

    #[test]
    fn error_code_no_specific_value() {
        for err in enum_iterator::all::<ExtError>() {
            let code = err.to_u32();
            assert_ne!(code, 0); // success code
        }
    }

    /// check forbidden error codes
    ///
    /// forbidden codes either:
    /// 1. never actually used
    /// 2. deprecated
    ///
    /// codes are forbidden to avoid collision in
    /// old programs that built their logic on these error codes
    /// if we accidentally re-use such codes
    #[test]
    fn error_codes_forbidden() {
        let codes = [
            0xffff, /* SyscallUsage */
            600,    /* ProgramRent(ProgramRentError::MaximumBlockCountPaid) */
        ];

        // check forbidden code is `Unsupported` variant now
        for code in codes {
            let err = ExtError::from_u32(code);
            assert_eq!(err, Some(ExtError::Unsupported));
        }

        // check forbidden code is never produced
        for err in enum_iterator::all::<ExtError>() {
            let code = err.to_u32();
            assert!(!codes.contains(&code));
        }
    }
}
