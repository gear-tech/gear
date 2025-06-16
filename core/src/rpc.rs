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

//! This module contains types commonly used as output in RPC calls.

use alloc::vec::Vec;
use gear_core_errors::ReplyCode;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Pre-calculated gas consumption estimate for a message.
///
/// Intended to be used as a result in `calculateGasFor*` RPC calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct GasInfo {
    /// The minimum amount of gas required for successful execution.
    pub min_limit: u64,
    /// The amount of gas that would be reserved.
    pub reserved: u64,
    /// The amount of gas that would be burned.
    pub burned: u64,
    /// The amount of gas that may be returned.
    pub may_be_returned: u64,
    /// Indicates whether the message was placed into the waitlist.
    ///
    /// This flag signifies that `min_limit` guarantees apply only to the first execution attempt.
    pub waited: bool,
}

/// Pre-calculated reply information.
///
/// Intended to be used as a result in `calculateReplyFor*` RPC calls.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyInfo {
    /// Payload of the reply.
    #[cfg_attr(feature = "std", serde(with = "impl_serde::serialize"))]
    pub payload: Vec<u8>,
    /// Value attached to the reply.
    pub value: u128,
    /// Reply code of the reply.
    pub code: ReplyCode,
}
