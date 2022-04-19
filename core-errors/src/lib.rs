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

use codec::{Decode, Encode};
use core::fmt;
use scale_info::TypeInfo;

/// Error using messages.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub enum MessageContextError {
    /// Message limit exceeded.
    LimitExceeded,
    /// Duplicate reply message.
    DuplicateReply,
    /// Duplicate waiting message.
    DuplicateWaiting,
    /// Duplicate waking message.
    DuplicateWaking,
    /// An attempt to commit or to push a payload into an already formed message.
    LateAccess,
    /// No message found with given handle, or handle exceeds the maximum messages amount.
    OutOfBounds,
    /// An attempt to push a payload into reply that was not set
    NoReplyFound,
    /// An attempt to interrupt execution with `wait(..)` while some messages weren't completed
    UncommittedPayloads,
    /// Duplicate init message
    DuplicateInit,
}

/// Memory error.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum MemoryError {
    /// Memory is over.
    ///
    /// All pages were previously allocated and there is nothing can be done.
    OutOfMemory,

    /// Allocation is in use.
    ///
    /// This is probably mis-use of the api (like dropping `Allocations` struct when some code is still runnig).
    AllocationsInUse,

    /// Specified page cannot be freed by the current program.
    ///
    /// It was allocated by another program.
    // TODO: WasmPageNumber
    InvalidFree(u32),

    /// Out of bounds memory access
    MemoryAccessError,

    /// There is wasm page, which has not all gear pages in the begin
    NotAllPagesInBegin,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TerminationReason {
    Exit,
    Leave,
    Wait,
    GasAllowance,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ExtError {
    Alloc(MemoryError),
    Free(MemoryError),
    Send(SendError),
    Reply(ReplyError),
    ExitTwice,
    GasLimitExceeded,
    TooManyGasAdded,
    TerminationReason(TerminationReason),
    Wake(MessageContextError),
    InitMessageNotDuplicated(MessageContextError),
}

impl fmt::Display for ExtError {
    fn fmt(&self, _fmt: &mut fmt::Formatter) -> fmt::Result {
        // TODO
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum SendError {
    MessageContext(MessageContextError),
    InsufficientMessageValue,
    GasLimitExceeded,
    NotEnoughValue,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ReplyError {
    MessageContext(MessageContextError),
    InsufficientMessageValue,
    NotEnoughValue,
}
