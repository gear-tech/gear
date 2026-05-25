// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gprimitives::{ActorId, U256};
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct TransferEvent {
    pub from: ActorId,
    pub to: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ApprovalEvent {
    pub owner: ActorId,
    pub spender: ActorId,
    pub value: U256,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum Event {
    Transfer(TransferEvent),
    Approval(ApprovalEvent),
}
