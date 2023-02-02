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
#[cfg(feature = "codec")]
use scale_info::TypeInfo;

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
    Unreachable,
}

/// Reply error
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
pub enum HandleReplyError {
    /// Execution error.
    #[display(fmt = "Execution error: {_0}")]
    Execution(SimpleExecutionError),
    /// Not executable actor.
    #[display(fmt = "Not executable actor")]
    NonExecutable,
    /// Message killed from storage as out of rent.
    #[display(fmt = "Out of rent")]
    OutOfRent,
    /// `gr_create_program` called with in-existing code ID.
    #[display(fmt = "Program code does not exist")]
    CodeNotExists,
}

/// Signal error
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo))]
pub enum SignalError {
    /// Execution error
    #[display(fmt = "Execution error: {_0}")]
    Execution(SimpleExecutionError),
    /// Message has been removed from the waitlist
    #[display(fmt = "Message has been removed from the waitlist")]
    RemovedFromWaitlist,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;

    #[test]
    fn assert_sizes() {
        assert!(HandleReplyError::encoded_fixed_size().unwrap() <= mem::size_of::<u32>());
        assert!(SignalError::encoded_fixed_size().unwrap() <= mem::size_of::<u32>());
    }
}
