// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Helper crate defining necessary types for messages to Gear built-in actor.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use scale_info::scale::{self, Decode, Encode};

pub type AccountId = [u8; 32];

/// Output of a built-in actor's `handle()` function to be encoded into a reply message.
pub type BuiltInActorResult = Result<RuntimeCallOutcome, error::BuiltInActorError>;

/// Output of an underlying runtime call.
pub type RuntimeCallOutcome = Result<u64, error::RuntimeCallErrorReason>;

pub mod error {
    use crate::staking::StakingErrorReason;

    use super::*;

    /// Error type representing errors in the built-in actor's `handle()` function itself, like
    /// decoding errors, insufficient resources etc.
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum BuiltInActorError {
        /// An error occurs if the underlying call has the weight greater than the `gas_limit`.
        #[display(fmt = "Not enough gas supplied")]
        InsufficientGas,
        /// An error occurs if the dispatch's message can't be decoded into a known type.
        #[display(fmt = "Failure to decode message")]
        UnknownMessageType,
    }

    /// Error type representing errors in the underlying runtime call.
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum RuntimeCallErrorReason {
        Staking(StakingErrorReason),
    }
}

pub mod staking {
    use super::*;

    /// Type that should be used to create a message to the staking built-in actor.
    /// It is a [partial] mirror of the staking pallet interface. Not all extrinsics
    /// are supported, more can be added as needed for real-world use cases.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum StakingMessage {
        /// Bond up to the `value` from the sender to self as the controller.
        Bond { value: u128 },
        /// Add up to the `value` to the sender's bonded amount.
        BondExtra { value: u128 },
        /// Unbond up to the `value` to allow withdrawal after undonding period.
        Unbond { value: u128 },
        /// Withdraw unbonded chunks for which undonding period has elapsed.
        WithdrawUnbonded { num_slashing_spans: u32 },
        /// Add sender as a nominator of `targets` or update the existing targets.
        Nominate { targets: Vec<AccountId> },
        /// Request stakers payout for the given era.
        PayoutStakers {
            validator_stash: AccountId,
            era: u32,
        },
        /// Rebond a portion of the sender's stash scheduled to be unlocked.
        Rebond { value: u128 },
    }

    /// Message processing output
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum StakingResponse {
        Success,
        Failure(StakingErrorReason),
    }

    impl<T> From<Result<T, StakingErrorReason>> for StakingResponse {
        fn from(result: Result<T, StakingErrorReason>) -> Self {
            result.map_or_else(StakingResponse::Failure, |_| StakingResponse::Success)
        }
    }

    /// Type mirroring the staking pallet extrinsics outcomes.
    // TODO: do we need more granularity here? Like one-to-one mapping from `pallet_staking::Error<T>`?
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum StakingErrorReason {
        #[display(fmt = "Dispatch internal error")]
        DispatchError,
    }
}
