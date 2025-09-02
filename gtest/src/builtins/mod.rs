// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Builtin actors implementations for gtest.

mod bls12_381;
mod eth_bridge;

pub use bls12_381::{BLS12_381_ID, Bls12_381Request, Bls12_381Response};
pub use eth_bridge::{ETH_BRIDGE_ID, EthBridgeRequest, EthBridgeResponse};

pub(crate) use bls12_381::process_bls12_381_dispatch;
pub(crate) use eth_bridge::process_eth_bridge_dispatch;

use core_processor::common::{ActorExecutionErrorReplyReason, TrapExplanation};
use gear_core::str::LimitedStr;
use parity_scale_codec::{Decode, Encode};

/// Builtin actor errors.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the
    /// `gas_limit`.
    InsufficientGas,
    /// Occurs if the dispatch's value is less than the minimum required value.
    InsufficientValue,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    DecodingError,
    /// Actor's inner error encoded as a String.
    Custom(LimitedStr<'static>),
    /// Occurs if a builtin actor execution does not fit in the current block.
    GasAllowanceExceeded,
}

impl From<BuiltinActorError> for ActorExecutionErrorReplyReason {
    /// Convert [`BuiltinActorError`] to
    /// [`core_processor::common::ActorExecutionErrorReplyReason`]
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
            }
            BuiltinActorError::InsufficientValue => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str("Not enough value supplied").into(),
                ))
            }
            BuiltinActorError::DecodingError => ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic(LimitedStr::from_small_str("Message decoding error").into()),
            ),
            BuiltinActorError::Custom(e) => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(e.into()))
            }
            BuiltinActorError::GasAllowanceExceeded => {
                unreachable!("Never supposed to be converted to error reply reason")
            }
        }
    }
}
