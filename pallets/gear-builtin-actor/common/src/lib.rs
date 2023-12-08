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
use gear_core_errors::SimpleExecutionError;
use scale_info::scale::{self, Decode, Encode};

pub type AccountId = [u8; 32];

pub use error::{BuiltInActorError, DispatchErrorReason};

/// Message processing output
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
#[codec(crate = scale)]
pub enum Response {
    Success,
    Failure(DispatchErrorReason),
}

impl<T> From<Result<T, DispatchErrorReason>> for Response {
    fn from(result: Result<T, DispatchErrorReason>) -> Self {
        result.map_or_else(Response::Failure, |_| Response::Success)
    }
}

pub mod error {
    use super::*;

    /// Built-in actor "own" errors (errors in `handle` function itself, like
    /// decoding errors, insufficient resources etc.)
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum BuiltInActorError {
        /// Occurs if the underlying call has the weight greater than the `gas_limit`.
        #[display(fmt = "Not enough gas supplied")]
        InsufficientGas,
        /// Occurs if the dispatch's message can't be decoded into a known type.
        #[display(fmt = "Failure to decode message")]
        UnknownMessageType,
    }

    impl From<BuiltInActorError> for SimpleExecutionError {
        /// Convert [`BuiltInActorError`] into [`gear_core_errors::SimpleExecutionError`].
        // TODO: should we think of a better mapping?
        fn from(err: BuiltInActorError) -> Self {
            match err {
                BuiltInActorError::InsufficientGas => SimpleExecutionError::RanOutOfGas,
                BuiltInActorError::UnknownMessageType => SimpleExecutionError::UserspacePanic,
            }
        }
    }

    /// Type representing a dispatched Runtime call internal error.
    // TODO: see if we can add more granularity to the error type (e.g. describe the pallet etc.)
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum DispatchErrorReason {
        #[display(fmt = "Runtime internal error")]
        RuntimeError,
    }
}

pub mod staking {
    use super::*;

    /// Type that should be used to create a message to the staking built-in actor.
    ///
    /// A [partial] mirror of the staking pallet interface. Not all extrinsics
    /// are supported, more can be added as needed for real-world use cases.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum Request {
        /// Bond up to the `value` from the sender to self as the controller.
        Bond {
            value: u128,
            payee: Option<RewardAccount>,
        },
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
        /// Set the reward destination.
        SetPayee { payee: RewardAccount },
    }

    /// An account where the rewards should accumulate on.
    ///
    /// In order to separate the contract's own balance from the rewards earned by users funds,
    /// a separate account for the rewards can be assigned.
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum RewardAccount {
        /// Accumulate the rewards on contract's account derived from its `program_id`.
        Program,
        /// Accumulate the rewards on a separate account.
        Custom(AccountId),
    }
}
