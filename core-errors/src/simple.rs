// Copyright (C) 2023 Gear Technologies Inc.
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

//! Simple errors being used for status codes

use enum_iterator::Sequence;

/// Type that can be encoded and decoded into status code
pub trait SimpleCodec: sealed::Sealed + Sized {
    /// Convert type into status code
    fn into_status_code(self) -> i32;

    /// Convert status code into self
    fn from_status_code(status_code: i32) -> Option<Self>;
}

mod sealed {
    pub trait Sealed {}
}

/// Simple execution error
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Sequence, derive_more::Display,
)]
#[non_exhaustive]
#[repr(u8)]
pub enum SimpleExecutionError {
    /// Gas limit exceeded
    #[display(fmt = "Gas limit exceeded")]
    GasLimitExceeded = 0,
    /// Memory exceeded
    #[display(fmt = "Memory exceeded")]
    MemoryExceeded = 1,
    /// Ext error
    #[display(fmt = "Business-logic error")]
    Ext = 2,
    /// Panic occurred
    Panic = 3,
    /// `unreachable` occurred
    Unknown = 255,
}

impl SimpleExecutionError {
    fn decode(num: u8) -> Option<Self> {
        match num {
            0 => Some(Self::GasLimitExceeded),
            1 => Some(Self::MemoryExceeded),
            2 => Some(Self::Ext),
            3 => Some(Self::Panic),
            255 => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Reply error
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Sequence, derive_more::Display,
)]
#[non_exhaustive]
#[repr(u8)]
pub enum SimpleReplyError {
    /// Execution error.
    #[display(fmt = "Execution error: {_0}")]
    Execution(SimpleExecutionError) = 1,
    /// Not executable actor.
    #[display(fmt = "Not executable actor")]
    NonExecutable = 2,
    /// Message killed from storage as out of rent.
    #[display(fmt = "Out of rent")]
    OutOfRent = 3,
    /// `gr_create_program` called with in-existing code ID.
    #[display(fmt = "Program code does not exist")]
    CodeNotExists = 4,
}

impl SimpleCodec for SimpleReplyError {
    fn into_status_code(self) -> i32 {
        let first = match self {
            SimpleReplyError::Execution(_) => 1,
            SimpleReplyError::NonExecutable => 2,
            SimpleReplyError::OutOfRent => 3,
            SimpleReplyError::CodeNotExists => 4,
        };
        let mut second = 0;
        if let Self::Execution(err) = self {
            second = err as u32;
        }

        (first | second << 8) as i32
    }

    fn from_status_code(status_code: i32) -> Option<Self> {
        let status_code = status_code as u32;
        let first = (status_code & 0xff) as u8;
        let second = ((status_code & 0xff00) >> 8) as u8;

        let execution = SimpleExecutionError::decode(second)?;

        match first {
            1 => Some(SimpleReplyError::Execution(execution)),
            2 => Some(SimpleReplyError::NonExecutable),
            3 => Some(SimpleReplyError::OutOfRent),
            4 => Some(SimpleReplyError::CodeNotExists),
            _ => None,
        }
    }
}

impl sealed::Sealed for SimpleReplyError {}

/// Signal error
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Sequence, derive_more::Display,
)]
#[non_exhaustive]
#[repr(u8)]
pub enum SimpleSignalError {
    /// Execution error
    #[display(fmt = "Execution error: {_0}")]
    Execution(SimpleExecutionError) = 1,
    /// Message has been removed from the waitlist
    #[display(fmt = "Message has been removed from the waitlist")]
    RemovedFromWaitlist = 2,
}

impl SimpleCodec for SimpleSignalError {
    fn into_status_code(self) -> i32 {
        let first = match self {
            SimpleSignalError::Execution(_) => 1,
            SimpleSignalError::RemovedFromWaitlist => 2,
        };
        let mut second = 0;
        if let Self::Execution(err) = self {
            second = err as u32;
        }

        (first | second << 8) as i32
    }

    fn from_status_code(status_code: i32) -> Option<Self> {
        let status_code = status_code as u32;
        let first = (status_code & 0xff) as u8;
        let second = ((status_code & 0xff00) >> 8) as u8;

        let execution = SimpleExecutionError::decode(second)?;

        match first {
            1 => Some(SimpleSignalError::Execution(execution)),
            2 => Some(SimpleSignalError::RemovedFromWaitlist),
            _ => None,
        }
    }
}

impl sealed::Sealed for SimpleSignalError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode() {
        for variant in enum_iterator::all::<SimpleSignalError>() {
            let status_code = variant.into_status_code();
            assert_ne!(variant.into_status_code(), 0);
            assert_eq!(
                SimpleSignalError::from_status_code(status_code),
                Some(variant)
            );
        }

        for variant in enum_iterator::all::<SimpleReplyError>() {
            let status_code = variant.into_status_code();
            assert_ne!(variant.into_status_code(), 0);
            assert_eq!(
                SimpleReplyError::from_status_code(status_code),
                Some(variant)
            );
        }
    }
}
