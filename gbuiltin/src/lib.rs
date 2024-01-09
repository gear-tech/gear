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

//! Helper crate defining Gear built-in actors communication protocol.
//!
//! This crate defines a set of types that contracts can use to interact
//! with the so-called "builtin" actors - that is the actors that are defined
//! for any Gear runtime and provide an API for the applications to build on top
//! of some blockchain logic like staking, governance, etc.

//! For a builtin actor to process a message, it should be able to decode its
//! payload into one of the supported message types.
//!
//! # Examples
//!
//! The following example shows how a contract can send a message to a builtin actor
//! (specifically, a staking actor) to bond some `value` to self as the controller
//! so that the contract can later use the staking API to nominate validators.
//!
//! ```ignore
//! use gstd::{msg, ActorId};
//! use gbuiltin::staking::{Request, RequestV1, RewardAccount};
//! use parity_scale_codec::Encode;
//!
//! const BUILT_IN: ActorId = ActorId::new(hex_literal::hex!(
//!     "9d765baea1938d17096421e4f881af7dc4ce5c15bb5022f409fc0d6265d97c3a"
//! ));
//!
//! #[gstd::async_main]
//! async fn main() {
//!     let value = msg::value();
//!     let payee: Option<RewardAccount> = None;
//!     let payload = Request::V1(RequestV1::Bond { value, payee }).encode();
//!     let _ = msg::send_bytes_for_reply(BUILT_IN, &payload[..], 0, 0)
//!         .expect("Error sending message")
//!         .await;
//! }
//! # fn main() {}
//! ```
//!

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use scale_info::scale::{self, Decode, Encode};

pub type AccountId = [u8; 32];

pub use error::DispatchErrorReason;

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

    /// Type representing a dispatched Runtime call internal error.
    // TODO: see if we can add more granularity to the error type (e.g. describe the pallet etc.)
    #[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
    #[codec(crate = scale)]
    pub enum DispatchErrorReason {
        #[display(fmt = "Runtime internal error")]
        RuntimeError,
    }
}

// TODO: review and adjust
pub mod staking {
    use super::*;

    /// Type that should be used to create a message to the staking built-in actor.
    ///
    /// A [partial] mirror of the staking pallet interface. Not all extrinsics
    /// are supported, more can be added as needed for real-world use cases.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum Request {
        /// Version 1 of the staking built-in actor protocol.
        V1(RequestV1),
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    #[codec(crate = scale)]
    pub enum RequestV1 {
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

// TODO: add more modules as new builtin actors are added
