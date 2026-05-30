// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gprimitives::{ActorId, U256};
use parity_scale_codec::{Decode, Encode};

/// Decoded representation of a WrappedVara ERC-20 `Transfer` log entry.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct TransferEvent {
    /// Sender of the tokens.
    pub from: ActorId,
    /// Recipient of the tokens.
    pub to: ActorId,
    /// Amount of tokens transferred, in the token's base unit.
    pub value: u128,
}

/// Decoded representation of a WrappedVara ERC-20 `Approval` log entry.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ApprovalEvent {
    /// Token holder granting the allowance.
    pub owner: ActorId,
    /// Address authorized to spend tokens on behalf of `owner`.
    pub spender: ActorId,
    /// Approved spending allowance.
    pub value: U256,
}

/// A decoded WrappedVara ERC-20 contract event, re-exported as `WVaraEvent` from the parent module.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum Event {
    /// A token transfer occurred.
    Transfer(TransferEvent),
    /// A spending allowance was set or updated.
    Approval(ApprovalEvent),
}
