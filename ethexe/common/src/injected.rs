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

use crate::ecdsa::Signature;
use alloc::vec::Vec;
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};

/// NOTE: resulting message_id is a hash of `(self.transaction, self.validity)`.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct SignedInjectedTransaction {
    /// Transaction itself.
    transaction: InjectedTransaction,
    /// Validity parameters.
    validity: ValidityParams,
    /// Signature over `(self.transaction, self.validity)` [aka message_id].
    signature: Signature,
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Destination program inside `Gear.exe`.
    destination: ActorId,
    /// Payload of the message.
    payload: Vec<u8>,
    /// Value attached to the message.
    ///
    /// NOTE: at this moment will be zero.
    value: u128,
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct ValidityParams {
    /// Reference block number.
    reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    ///
    /// NOTE: this is also a salt for MessageId generation.
    salt: Vec<u8>,
}
