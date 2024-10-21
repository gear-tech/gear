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
use bitflags::bitflags;
use gear_wasm_instrument::syscalls::SyscallName;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Bitflag contains entry points
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct DispatchKind(u8);

bitflags! {
    impl DispatchKind: u8 {
        /// Initialization.
        const Init = 0b0001;
        /// Common handle.
        const Handle = 0b0010;
        /// Handle reply.
        const Reply = 0b0100;
        /// System signal.
        const Signal = 0b1000;
    }
}

impl DispatchKind {
    /// Syscalls that are not allowed to be called for the dispatch kind.
    pub fn forbidden_funcs(&self) -> BTreeSet<SyscallName> {
        match self {
            s if s.contains(DispatchKind::Signal) => [
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

impl Default for DispatchKind {
    fn default() -> Self {
        DispatchKind::Handle
    }
}

impl WasmEntryPoint for DispatchKind {
    fn as_entry(&self) -> &str {
        match *self {
            DispatchKind::Init => "init",
            DispatchKind::Handle => "handle",
            DispatchKind::Reply => "handle_reply",
            DispatchKind::Signal => "handle_signal",
            _ => unreachable!("Multiple dispatch kinds are not allowed"),
        }
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        let kind = match entry {
            "init" => DispatchKind::Init,
            "handle" => DispatchKind::Handle,
            "handle_reply" => DispatchKind::Reply,
            "handle_signal" => DispatchKind::Signal,
            _ => return None,
        };

        Some(kind)
    }
}
