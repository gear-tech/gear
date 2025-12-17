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

use crate::{Address, HashOf, ToDigest, ecdsa::SignedData};
use core::hash::Hash;
use gear_core::rpc::ReplyInfo;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::{Digest, Keccak256};
use sp_core::Bytes;

/// Recent block hashes window size used to check transaction mortality.
pub const VALIDITY_WINDOW: u8 = 32;

pub type SignedInjectedTransaction = SignedData<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct RpcOrNetworkInjectedTx {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    pub tx: SignedInjectedTransaction,
}

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Destination program inside `Vara.eth`.
    pub destination: ActorId,
    /// Payload of the message.
    pub payload: Bytes,
    /// Value attached to the message.
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    /// NOTE: this is also a salt for MessageId generation.
    pub salt: Bytes,
}

impl ToDigest for InjectedTransaction {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            destination,
            payload,
            value,
            reference_block,
            salt,
        } = self;

        destination.into_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
        value.to_be_bytes().update_hasher(hasher);
        reference_block.0.update_hasher(hasher);
        salt.update_hasher(hasher);
    }
}

impl InjectedTransaction {
    /// Returns the hash of [`InjectedTransaction`].
    pub fn to_hash(&self) -> HashOf<InjectedTransaction> {
        // Safe because we hash corresponding type itself
        let bytes = [
            self.destination.as_ref(),
            self.payload.as_ref(),
            &self.value.to_be_bytes(),
            &self.reference_block.0,
            self.salt.as_ref(),
        ]
        .concat();
        unsafe { HashOf::new(gear_core::utils::hash(&bytes).into()) }
    }

    /// Creates [`MessageId`] from [`InjectedTransaction`].
    pub fn to_message_id(&self) -> MessageId {
        MessageId::new(self.to_hash().inner().0)
    }
}

/// [`Promise`] represents the guaranteed reply for [`InjectedTransaction`].
///
/// Note: Validator must ensure the validity of the promise, because of it can be slashed for
/// providing an invalid promise.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct Promise {
    /// Hash of the injected transaction this reply corresponds to.
    pub tx_hash: HashOf<InjectedTransaction>,
    /// Reply data for injected message.
    pub reply: ReplyInfo,
}

/// Signed wrapper on top of [`Promise`].
/// It will be shared among other validators as a proof of promise.
pub type SignedPromise = SignedData<Promise>;

impl ToDigest for Promise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self { tx_hash, reply } = self;

        hasher.update(tx_hash.inner());
        reply.update_hasher(hasher);
    }
}

// Injected transaction validity status.

/// The status of [`InjectedTransaction`] for specific announce and chain head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxValidity {
    /// Transaction is valid and can be include into announce.
    Valid,
    /// Transaction is in intermediate status ([`TxValidityIntermediateStatus`]).
    Intermediate(TxValidityIntermediateStatus),
    /// Transaction is not valid.
    /// The [`TxRejection`] will be returned to the transaction's sender.
    Invalid(TxInvalidityStatus),
}

/// The intermediate status means that the transaction is not valid now, but
/// it may become valid in the future (e.g., after a reorg).
///
/// In this status, the transaction should be kept in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum TxValidityIntermediateStatus {
    #[display("Transaction's reference block is not on current branch")]
    NotOnCurrentBranch,
    /// In case when transaction is sent to uninitialized actor, we keep it in pool,
    /// because in next blocks actor can be initialized.
    #[display("Transaction's destination actor is uninitialized")]
    UninitializedDestination,
}

/// Represents the rejection of injected transaction.
/// This object will be sent back to the transaction's sender.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("Transaction({tx_hash}) was rejected because of: {reason}")]
pub struct TxRejection {
    pub tx_hash: HashOf<InjectedTransaction>,
    pub reason: TxInvalidityStatus,
}

/// The reason why the transaction is not valid and cannot be included into announce.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum TxInvalidityStatus {
    #[display("Transaction with the same hash was already included")]
    Duplicate,
    #[display("Transaction was not included within validity window and becomes outdated")]
    Outdated,
    #[display("Transaction's destination actor({destination}) not found")]
    UnknownDestination { destination: gprimitives::ActorId },
    #[display("Transaction's destination actor({destination}) is uninitialized")]
    UninitializedDestination { destination: gprimitives::ActorId },
}
