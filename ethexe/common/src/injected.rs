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

use crate::{
    Address, Announce, HashOf, ProgramStates, ToDigest,
    db::{AnnounceStorageRO, OnChainStorageRO},
    ecdsa::SignedData,
};
use anyhow::{Result, anyhow};
use core::hash::Hash;
use gear_core::rpc::ReplyInfo;
use gprimitives::{ActorId, H256, MessageId};
use hashbrown::HashSet;
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;
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
        unsafe { HashOf::new(gear_core::utils::hash(&self.encode()).into()) }
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

        tx_hash.update_hasher(hasher);
        reply.update_hasher(hasher);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TxValidity {
    /// Transaction is valid and can be include into announce.
    Valid,
    /// Transaction was already include into one of previous [`VALIDITY_WINDOW`] announces.
    Duplicate,
    /// Transaction is outdated and should be remove from pool.
    Outdated,
    /// Transaction's reference block not on current branch.
    /// Keep tx in pool in case of reorg.
    NotOnCurrentBranch,
    /// Transaction's destination [`gprimitives::ActorId`] not found.
    UnknownDestination,
}

pub struct TxValidityChecker<DB> {
    db: DB,
    chain_head: H256,
    recent_included_txs: HashSet<HashOf<InjectedTransaction>>,
    latest_states: ProgramStates,
}

impl<DB: OnChainStorageRO + AnnounceStorageRO> TxValidityChecker<DB> {
    pub fn new_for_announce(db: DB, chain_head: H256, announce: HashOf<Announce>) -> Result<Self> {
        Ok(Self {
            recent_included_txs: Self::collect_recent_included_txs(&db, announce)?,
            latest_states: db.announce_program_states(announce).unwrap_or_default(),
            db,
            chain_head,
        })
    }

    /// To determine the validity of transaction is enough to check the validity of its reference block.
    pub fn check_tx_validity(&self, tx: &SignedInjectedTransaction) -> Result<TxValidity> {
        let reference_block = tx.data().reference_block;

        if !self.is_reference_block_within_validity_window(reference_block)? {
            return Ok(TxValidity::Outdated);
        }

        if !self.is_reference_block_on_current_branch(reference_block)? {
            return Ok(TxValidity::NotOnCurrentBranch);
        }

        if self.recent_included_txs.contains(&tx.data().to_hash()) {
            return Ok(TxValidity::Duplicate);
        }

        if !self.latest_states.contains_key(&tx.data().destination) {
            return Ok(TxValidity::UnknownDestination);
        }

        Ok(TxValidity::Valid)
    }

    fn is_reference_block_within_validity_window(&self, reference_block: H256) -> Result<bool> {
        let reference_block_height = self
            .db
            .block_header(reference_block)
            .ok_or_else(|| anyhow!("Block header not found for reference block {reference_block}"))?
            .height;

        let chain_head_height = self
            .db
            .block_header(self.chain_head)
            .ok_or_else(|| anyhow!("Block header not found for hash: {}", self.chain_head))?
            .height;

        Ok(reference_block_height <= chain_head_height
            && reference_block_height + VALIDITY_WINDOW as u32 > chain_head_height)
    }

    // TODO #4808: branch check must be until genesis block
    fn is_reference_block_on_current_branch(&self, reference_block: H256) -> Result<bool> {
        let mut block_hash = self.chain_head;
        for _ in 0..VALIDITY_WINDOW {
            if block_hash == reference_block {
                return Ok(true);
            }

            block_hash = self
                .db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }

    pub fn collect_recent_included_txs(
        db: &DB,
        announce: HashOf<Announce>,
    ) -> Result<HashSet<HashOf<InjectedTransaction>>> {
        let mut txs = HashSet::new();

        let mut announce_hash = announce;
        for _ in 0..VALIDITY_WINDOW {
            let Some(announce) = db.announce(announce_hash) else {
                // Reach genesis_announce - correct case.
                if announce_hash == HashOf::zero() {
                    break;
                }

                // TODO: #4969 temporary hack ignoring this error for fast_sync test.
                // Reach start announce is not correct case, because of can exists earlier announces with injected txs.
                // anyhow::bail!("Reaching start announce is not supported; decrease VALIDITY_WINDOW")
                break;
            };

            announce_hash = announce.parent;

            txs.extend(
                announce
                    .injected_transactions
                    .into_iter()
                    .map(|tx| tx.data().to_hash()),
            );
        }

        Ok(txs)
    }
}
