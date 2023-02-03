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

#[cfg(feature = "codec")]
use codec::{Decode, Encode};
use core::{iter, mem};
#[cfg(feature = "codec")]
use scale_info::TypeInfo;

/// Type that can be encoded and decoded into status code
pub trait SimpleCodec: Encode + Decode + sealed::Sealed + Sized {
    /// Convert type into status code
    fn into_status_code(self) -> i32 {
        const U32_SIZE: usize = mem::size_of::<i32>();

        let mut buf = self.encode();
        assert!(buf.len() <= U32_SIZE);
        buf.extend(iter::repeat(0).take(U32_SIZE - buf.len()));
        let buf = buf.try_into().expect("Vec must be exactly 4 bytes length");
        assert_ne!(
            buf, [0; 4],
            "Encoded simple error shouldn't be 0 because it's successful status code"
        );

        u32::from_le_bytes(buf) as i32
    }

    /// Convert status code into self
    fn from_status_code(status_code: i32) -> Option<Self> {
        let status_code = status_code as u32;
        let status_code = status_code.to_le_bytes();
        Self::decode(&mut status_code.as_ref()).ok()
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Simple execution error
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
pub enum SimpleExecutionError {
    /// Gas limit exceeded
    #[display(fmt = "Gas limit exceeded")]
    GasLimitExceeded,
    /// Memory exceeded
    #[display(fmt = "Memory exceeded")]
    MemoryExceeded,
    /// Ext error
    #[display(fmt = "Business-logic error")]
    Ext,
    /// Panic occurred
    Panic,
    /// `unreachable` occurred
    Unknown,
}

/// Reply error
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[repr(u8)]
#[allow(clippy::unnecessary_cast)]
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

impl SimpleCodec for SimpleReplyError {}
impl sealed::Sealed for SimpleReplyError {}

/// Signal error
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
#[repr(u8)]
#[allow(clippy::unnecessary_cast)]
pub enum SimpleSignalError {
    /// Execution error
    #[display(fmt = "Execution error: {_0}")]
    Execution(SimpleExecutionError) = 1,
    /// Message has been removed from the waitlist
    #[display(fmt = "Message has been removed from the waitlist")]
    RemovedFromWaitlist = 2,
}

impl SimpleCodec for SimpleSignalError {}
impl sealed::Sealed for SimpleSignalError {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;

    #[test]
    fn assert_sizes() {
        assert!(SimpleReplyError::encoded_fixed_size().unwrap() <= mem::size_of::<u32>());
        assert!(SimpleSignalError::encoded_fixed_size().unwrap() <= mem::size_of::<u32>());
    }
}
