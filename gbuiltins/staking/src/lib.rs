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
//! use gbuiltins::staking::{Request, RewardAccount};
//! use parity_scale_codec::Encode;
//!
//! const BUILTIN_ADDRESS: ActorId = ActorId::new(hex_literal::hex!(
//!     "77f65ef190e11bfecb8fc8970fd3749e94bed66a23ec2f7a3623e785d0816761"
//! ));
//!
//! #[gstd::async_main]
//! async fn main() {
//!     let value = msg::value();
//!     let payee: RewardAccount = RewardAccount::Program;
//!     let payload = Request::Bond { value, payee }.encode();
//!     let _ = msg::send_bytes_for_reply(BUILTIN_ADDRESS, &payload[..], 0, 0)
//!         .expect("Error sending message")
//!         .await;
//! }
//! # fn main() {}
//! ```
//!

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Type that should be used to create a message to the staking built-in actor.
///
/// A `partial` mirror of the staking pallet interface. Not all extrinsics
/// are supported, more can be added as needed for real-world use cases.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub enum Request {
    /// Bond up to the `value` from the sender to self as the controller.
    #[codec(index = 0)]
    Bond { value: u128, payee: RewardAccount },

    /// Add up to the `value` to the sender's bonded amount.
    #[codec(index = 1)]
    BondExtra { value: u128 },

    /// Unbond up to the `value` to allow withdrawal after undonding period.
    #[codec(index = 2)]
    Unbond { value: u128 },

    /// Withdraw unbonded chunks for which undonding period has elapsed.
    #[codec(index = 3)]
    WithdrawUnbonded { num_slashing_spans: u32 },

    /// Add sender as a nominator of `targets` or update the existing targets.
    #[codec(index = 4)]
    Nominate { targets: Vec<ActorId> },

    /// Declare intention to `temporarily` stop nominating while still having funds bonded.
    #[codec(index = 5)]
    Chill,

    /// Request stakers payout for the given era.
    #[codec(index = 6)]
    PayoutStakers { validator_stash: ActorId, era: u32 },

    /// Rebond a portion of the sender's stash scheduled to be unlocked.
    #[codec(index = 7)]
    Rebond { value: u128 },

    /// Set the reward destination.
    #[codec(index = 8)]
    SetPayee { payee: RewardAccount },

    /// Get the active era.
    #[codec(index = 9)]
    ActiveEra,
}

/// An account where the rewards should accumulate on.
///
/// A "mirror" of the staking pallet's `RewardDestination` enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub enum RewardAccount {
    /// Pay rewards to the sender's account and increase the amount at stake.
    Staked,
    /// Pay rewards to the sender's account (usually, the one derived from `program_id`)
    /// without increasing the amount at stake.
    Program,
    /// Pay rewards to a custom account.
    Custom(ActorId),
    /// Opt for not receiving any rewards at all.
    None,
}

/// Response type for staking built-in actor operations.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub enum Response {
    /// Response containing the active era details amd when it was executed.
    ActiveEra {
        // Current active era index.
        index: u32,
        /// Moment of start expressed as millisecond from `$UNIX_EPOCH`.
        ///
        /// Start can be none if start hasn't been set for the era yet,
        /// Start is set on the first on_finalize of the era to guarantee usage of `Time`.
        start: Option<u64>,
        // Block number when the request was executed.
        executed_at: u32,
        // Gear block number when the request was executed.
        executed_at_gear_block: u32,
    },
}
