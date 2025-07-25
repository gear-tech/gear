// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Types used to communicate with proxy built-in.

#![no_std]

use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Request that can be handled by the proxy builtin.
///
/// Currently all proxies aren't required to send announcement,
/// i.e. no delays for the delegate actions.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum Request {
    /// Add proxy request.
    ///
    /// Requests to add `delegate` as a delegate for the actions
    /// defined by `proxy_type` to be done on behalf of the request
    /// sender.
    #[codec(index = 0)]
    AddProxy {
        delegate: ActorId,
        proxy_type: ProxyType,
    },
    /// Remove proxy request.
    ///
    /// Request sender asks to remove `delegate` with set of allowed actions
    /// defined in `proxy_type` from his list of proxies.
    #[codec(index = 1)]
    RemoveProxy {
        delegate: ActorId,
        proxy_type: ProxyType,
    },
}

/// Proxy type.
///
/// The mirror enum for the one defined in vara-runtime crate.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ProxyType {
    Any,
    NonTransfer,
    Governance,
    Staking,
    IdentityJudgement,
    CancelProxy,
}
