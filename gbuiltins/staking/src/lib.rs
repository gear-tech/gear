// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
//! use gbuiltins::staking::{Request, RequestV1, RewardAccount};
//! use parity_scale_codec::Encode;
//!
//! const BUILTIN_ADDRESS: ActorId = ActorId::new(hex_literal::hex!(
//!     "77f65ef190e11bfecb8fc8970fd3749e94bed66a23ec2f7a3623e785d0816761"
//! ));
//!
//! #[gstd::async_main]
//! async fn main() {
//!     let value = msg::value();
//!     let payee: Option<RewardAccount> = None;
//!     let payload = Request::V1(RequestV1::Bond { value, payee }).encode();
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
use scale_info::scale::{self, Decode, Encode};

pub type AccountId = [u8; 32];

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
    /// Declare intention to [temporarily] stop nominating while still having funds bonded.
    Chill,
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

/// Message processing output
pub type Response = ();

/// Type representing a dispatched Runtime call internal error.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
#[codec(crate = scale)]
pub enum DispatchErrorReason {
    #[display(fmt = "Runtime internal error")]
    RuntimeError,
}
