// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

use crate::message::WasmEntryPoint;
use alloc::collections::BTreeSet;
use enumflags2::{bitflags, BitFlags};
use gear_wasm_instrument::syscalls::SyscallName;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Bitflags contains entry points
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct DispatchKindSet(u8);

impl From<BitFlags<DispatchKind>> for DispatchKindSet {
    fn from(flags: BitFlags<DispatchKind>) -> Self {
        Self(flags.bits())
    }
}

impl DispatchKindSet {
    /// Create empty flags.
    pub fn empty() -> Self {
        Self(0)
    }

    /// Convert to bitflags.
    pub fn as_flags(self) -> BitFlags<DispatchKind> {
        BitFlags::from_bits(self.0).unwrap()
    }
}

/// Bitflags contains entry points
#[bitflags(default = Handle)]
#[repr(u8)]
#[derive(
    Copy, Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
pub enum DispatchKind {
    /// Initialization.
    Init = 0b0001,
    /// Common handle.
    #[default]
    Handle = 0b0010,
    /// Handle reply.
    Reply = 0b0100,
    /// System signal.
    Signal = 0b1000,
}

impl DispatchKind {
    /// Check if kind is init.
    pub fn is_init(&self) -> bool {
        matches!(self, Self::Init)
    }

    /// Check if kind is handle.
    pub fn is_handle(&self) -> bool {
        matches!(self, Self::Handle)
    }

    /// Check if kind is reply.
    pub fn is_reply(&self) -> bool {
        matches!(self, Self::Reply)
    }

    /// Check if kind is signal.
    pub fn is_signal(&self) -> bool {
        matches!(self, Self::Signal)
    }

    /// Syscalls that are not allowed to be called for the dispatch kind.
    pub fn forbidden_funcs(&self) -> BTreeSet<SyscallName> {
        match self {
            DispatchKind::Signal => [
                SyscallName::Source,
                SyscallName::Reply,
                SyscallName::ReplyPush,
                SyscallName::ReplyCommit,
                SyscallName::ReplyCommitWGas,
                SyscallName::ReplyInput,
                SyscallName::ReplyInputWGas,
                SyscallName::ReservationReply,
                SyscallName::ReservationReplyCommit,
                SyscallName::SystemReserveGas,
            ]
            .into(),
            _ => Default::default(),
        }
    }
}

impl WasmEntryPoint for DispatchKind {
    fn as_entry(&self) -> &str {
        match *self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::Reply => "handle_reply",
            Self::Signal => "handle_signal",
        }
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        let kind = match entry {
            "init" => Self::Init,
            "handle" => Self::Handle,
            "handle_reply" => Self::Reply,
            "handle_signal" => Self::Signal,
            _ => return None,
        };

        Some(kind)
    }
}
