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
    DEFAULT_BLOCK_GAS_LIMIT, HashOf, ToDigest,
    db::InjectedStorageRW,
    events::BlockEvent,
    injected::{AnnounceInjectedTransaction, InjectedTransaction, SignedInjectedTransaction},
};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use core::ops::Not;
use gear_core::{ids::prelude::CodeIdExt as _, utils};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use gsigner::Signature;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sha3::Digest as _;

pub type ProgramStates = BTreeMap<ActorId, StateHashWithQueueSize>;

#[derive(Debug, Clone, Copy, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

impl BlockHeader {
    pub fn dummy(height: u32) -> Self {
        let mut parent_hash = [0; 32];
        parent_hash[..4].copy_from_slice(&height.to_le_bytes());

        Self {
            height,
            timestamp: height as u64 * 12,
            parent_hash: parent_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header,
        }
    }
}

#[derive(
    Debug, derive_more::Display, Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Default,
)]
#[display("Block(hash: {hash}, height: {}, parent: {}, ts: {})", header.height, header.parent_hash, header.timestamp)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, derive_more::Display)]
#[display(
    "Announce(block: {block_hash}, parent: {parent}, gas: {gas_allowance:?}, txs: {injected_transactions:?})"
)]
pub struct Announce {
    pub block_hash: H256,
    pub parent: HashOf<Self>,
    pub gas_allowance: Option<u64>,
    pub injected_transactions: Vec<AnnounceInjectedTransaction>,
}

impl Announce {
    pub fn to_hash(&self) -> HashOf<Self> {
        // # Safety because of implementation
        let Announce {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions,
        } = self;

        let transactions = injected_transactions
            .iter()
            .map(|tx| (*tx.signature(), tx.tx_hash()))
            .collect::<Vec<_>>();

        // NOTE: we use here the fact that None is encoding similar to empty vector:
        // None -> 0x00
        // vec![] -> 0x00
        let maybe_transactions_hash = transactions
            .is_empty()
            .not()
            .then(|| utils::hash(&transactions.encode()));

        let announce_parts = (block_hash, parent, gas_allowance, maybe_transactions_hash);
        unsafe { HashOf::new(H256(utils::hash(&announce_parts.encode()))) }
    }

    pub fn base(block_hash: H256, parent: HashOf<Self>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: None,
            injected_transactions: Vec::new(),
        }
    }

    pub fn with_default_gas(block_hash: H256, parent: HashOf<Self>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
            injected_transactions: Vec::new(),
        }
    }

    pub fn is_base(&self) -> bool {
        self.gas_allowance.is_none() && self.injected_transactions.is_empty()
    }
}

impl ToDigest for Announce {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash);
        hasher.update(self.gas_allowance.encode());
        hasher.update(self.injected_transactions.encode());
    }
}

/// [NetworkAnnounce] is the transport represenstation of [Announce].
///
/// It is designed to keep the [Announce] a lighweight struct wihout any
/// heavy dependencies.
/// [NetworkAnnounce] is used for transport the [Announce] with [InjectedTransaction] bodies.
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct NetworkAnnounce {
    pub block_hash: H256,
    pub parent: HashOf<Announce>,
    pub gas_allowance: Option<u64>,
    /// Full [InjectedTransaction] bodies.
    pub injected_transactions: Vec<SignedInjectedTransaction>,
}

impl NetworkAnnounce {
    pub fn base(block_hash: H256, parent: HashOf<Announce>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: None,
            injected_transactions: Vec::new(),
        }
    }

    pub fn with_default_gas(block_hash: H256, parent: HashOf<Announce>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
            injected_transactions: Vec::new(),
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn try_from_announce(
        announce: Announce,
        injected_transactions: Vec<SignedInjectedTransaction>,
    ) -> Result<Self, NetworkAnnounceFromAnnounceError> {
        (announce, injected_transactions).try_into()
    }

    pub fn to_hash(&self) -> HashOf<Announce> {
        Announce::from(self).to_hash()
    }

    /// Converts the [NetworkAnnounce] into an [Announce] and sets the injected transactions in the database.
    /// Guarantees that the injected transactions are persisted in the database.
    pub fn into_announce_persisting_injected_transactions<DB: ?Sized + InjectedStorageRW>(
        self,
        db: &DB,
    ) -> Announce {
        let Self {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions,
        } = self;

        let mut injected_transaction_hashes = Vec::with_capacity(injected_transactions.len());
        for tx in injected_transactions {
            injected_transaction_hashes.push(AnnounceInjectedTransaction::from_signed_tx(&tx));
            db.set_injected_transaction(tx);
        }

        Announce {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions: injected_transaction_hashes,
        }
    }

    /// Splits the [NetworkAnnounce] into an [Announce] and a vector of [SignedInjectedTransaction] bodies.
    pub fn split_into_parts(self) -> (Announce, Vec<SignedInjectedTransaction>) {
        let announce = Announce::from(&self);
        (announce, self.injected_transactions)
    }
}

impl From<&NetworkAnnounce> for Announce {
    fn from(network_announce: &NetworkAnnounce) -> Self {
        Self {
            block_hash: network_announce.block_hash,
            parent: network_announce.parent,
            gas_allowance: network_announce.gas_allowance,
            injected_transactions: network_announce
                .injected_transactions
                .iter()
                .map(AnnounceInjectedTransaction::from_signed_tx)
                .collect(),
        }
    }
}

impl From<NetworkAnnounce> for Announce {
    fn from(network_announce: NetworkAnnounce) -> Self {
        Self {
            block_hash: network_announce.block_hash,
            parent: network_announce.parent,
            gas_allowance: network_announce.gas_allowance,
            injected_transactions: network_announce
                .injected_transactions
                .into_iter()
                .map(|tx| AnnounceInjectedTransaction::from_signed_tx(&tx))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum NetworkAnnounceFromAnnounceError {
    #[display(
        "injected transactions count mismatch: announce has {announce_len}, provided {provided_len}"
    )]
    InjectedTransactionsLenMismatch {
        announce_len: usize,
        provided_len: usize,
    },
    #[display(
        "injected transaction hash mismatch at index {index}: expected {expected}, got {actual}"
    )]
    InjectedTransactionHashMismatch {
        index: usize,
        expected: HashOf<InjectedTransaction>,
        actual: HashOf<InjectedTransaction>,
    },
    #[display(
        "injected transaction signature mismatch at index {index}: expected {expected}, got {actual}"
    )]
    InjectedTransactionSignatureMismatch {
        index: usize,
        expected: Signature,
        actual: Signature,
    },
}

#[cfg(feature = "std")]
impl std::error::Error for NetworkAnnounceFromAnnounceError {}

impl TryFrom<(Announce, Vec<SignedInjectedTransaction>)> for NetworkAnnounce {
    type Error = NetworkAnnounceFromAnnounceError;

    fn try_from(
        (announce, injected_transactions): (Announce, Vec<SignedInjectedTransaction>),
    ) -> Result<Self, Self::Error> {
        let Announce {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions: announce_injected_transactions,
        } = announce;

        if announce_injected_transactions.len() != injected_transactions.len() {
            return Err(
                NetworkAnnounceFromAnnounceError::InjectedTransactionsLenMismatch {
                    announce_len: announce_injected_transactions.len(),
                    provided_len: injected_transactions.len(),
                },
            );
        }

        for (index, (expected, tx)) in announce_injected_transactions
            .iter()
            .zip(&injected_transactions)
            .enumerate()
        {
            let actual = tx.data().to_hash();
            if expected.tx_hash() != actual {
                return Err(
                    NetworkAnnounceFromAnnounceError::InjectedTransactionHashMismatch {
                        index,
                        expected: expected.tx_hash(),
                        actual,
                    },
                );
            }

            if *expected.signature() != *tx.signature() {
                return Err(
                    NetworkAnnounceFromAnnounceError::InjectedTransactionSignatureMismatch {
                        index,
                        expected: *expected.signature(),
                        actual: *tx.signature(),
                    },
                );
            }
        }

        Ok(Self {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions,
        })
    }
}

impl TryFrom<Announce> for NetworkAnnounce {
    type Error = NetworkAnnounceFromAnnounceError;

    fn try_from(announce: Announce) -> Result<Self, Self::Error> {
        Self::try_from_announce(announce, Vec::new())
    }
}

impl ToDigest for NetworkAnnounce {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash);
        hasher.update(self.gas_allowance.encode());
        hasher.update(self.injected_transactions.encode());
    }
}

/// [`PromisePolicy`] tells processor whether should it emits promises or not.
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Encode, Decode, derive_more::IsVariant)]
pub enum PromisePolicy {
    /// Emits promises in execution process.
    Enabled,
    // Do not emit promises in execution process.
    #[default]
    Disabled,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Default, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
pub struct StateHashWithQueueSize {
    pub hash: H256,
    pub canonical_queue_size: u8,
    pub injected_queue_size: u8,
}

impl StateHashWithQueueSize {
    pub fn zero() -> Self {
        Self {
            hash: H256::zero(),
            canonical_queue_size: 0,
            injected_queue_size: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub struct CodeBlobInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndIdUnchecked {
    #[debug("{:#x} bytes", code.len())]
    pub code: Vec<u8>,
    pub code_id: CodeId,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndId {
    #[debug("{:#x} bytes", code.len())]
    code: Vec<u8>,
    code_id: CodeId,
}

impl CodeAndId {
    pub fn new(code: Vec<u8>) -> Self {
        let code_id = CodeId::generate(&code);
        Self { code, code_id }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Creates a new `CodeAndId` from an unchecked version, asserting that the `code_id` matches the generated one.
    /// # Panics
    ///
    /// If the `code_id` does not match the generated one from the `code`, this function will panic.
    pub fn from_unchecked(code_and_id: CodeAndIdUnchecked) -> Self {
        let CodeAndIdUnchecked { code, code_id } = code_and_id;
        assert_eq!(
            code_id,
            CodeId::generate(&code),
            "CodeId does not match the provided code"
        );
        Self { code, code_id }
    }

    pub fn into_unchecked(self) -> CodeAndIdUnchecked {
        CodeAndIdUnchecked {
            code: self.code,
            code_id: self.code_id,
        }
    }
}

/// GearExe network timelines configuration. Parameters fetched the Router contract.
/// This struct stores in the database, because of using in the multiple places.
///
/// TODO(kuzmindev): `ProtocolTimelines` can store more protocol parameters,
/// for example `max_validators` in election.
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ProtocolTimelines {
    // The genesis timestamp of the GearExe network in seconds.
    pub genesis_ts: u64,
    // The duration of an era in seconds.
    pub era: u64,
    /// The election duration in seconds before the end of an era when the next set of validators elected.
    ///  (start of era)[ - - - - - - - - - - -  + - - - - ] (end of era)
    ///                                         ^ election
    pub election: u64,
    /// The slot duration in seconds.
    pub slot: u64,
}

// TODO: #5290 remove panics here
impl ProtocolTimelines {
    /// Returns the era index for the given timestamp. Eras starts from 0.
    ///
    /// # Panics
    /// If the given timestamp is less than `genesis_ts`, this function will panic.
    #[inline(always)]
    pub fn era_from_ts(&self, ts: u64) -> u64 {
        ts.checked_sub(self.genesis_ts)
            .expect("timestamp must be >= genesis_ts")
            / self.era
    }

    /// Returns the timestamp since which the given era started.
    #[inline(always)]
    pub fn era_start_ts(&self, era_index: u64) -> u64 {
        self.genesis_ts + era_index * self.era
    }

    /// Returns the timestamp when election starts in the given era.
    /// NOTE: election starts for the next era validators.
    #[inline(always)]
    pub fn era_election_start_ts(&self, era_index: u64) -> u64 {
        self.era_start_ts(era_index + 1) - self.election
    }

    /// Returns the slot index for the given timestamp. Slots starts from 0.
    ///
    /// # Panics
    /// If the given timestamp is less than `genesis_ts`, this function will panic.
    #[inline(always)]
    pub fn slot_from_ts(&self, ts: u64) -> u64 {
        ts.checked_sub(self.genesis_ts)
            .expect("timestamp must be >= genesis_ts")
            / self.slot
    }
}

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ActorId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ActorId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ActorId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

/// Scheduler; (block height, scheduled task)
pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{InjectedStorageRO, InjectedStorageRW},
        injected::InjectedTransaction,
    };
    use gsigner::PrivateKey;
    use std::{cell::RefCell, vec};

    #[test]
    fn test_era_from_ts_calculation() {
        let timelines = ProtocolTimelines {
            genesis_ts: 10,
            era: 234,
            election: 200,
            slot: 10,
        };

        // For 0 era
        assert_eq!(timelines.era_from_ts(10), 0);
        assert_eq!(timelines.era_from_ts(45), 0);
        assert_eq!(timelines.era_from_ts(243), 0);

        // For 1 era
        assert_eq!(timelines.era_from_ts(244), 1);
        assert_eq!(timelines.era_from_ts(333), 1);
    }

    #[should_panic(expected = "timestamp must be >= genesis_ts")]
    #[test]
    fn panic_on_era_from_ts_before_genesis() {
        ProtocolTimelines {
            genesis_ts: 100,
            era: 234,
            election: 200,
            slot: 10,
        }
        .era_from_ts(50);
    }

    #[test]
    fn test_era_start_calculation() {
        let timelines = ProtocolTimelines {
            genesis_ts: 10,
            era: 234,
            election: 200,
            slot: 10,
        };

        // For 0 era
        assert_eq!(timelines.era_start_ts(0), 10);
        assert_eq!(timelines.era_start_ts(0), 10);
        assert_eq!(timelines.era_start_ts(0), 10);

        // For 1 era
        assert_eq!(timelines.era_start_ts(1), 244);
        assert_eq!(timelines.era_start_ts(1), 244);
    }

    fn make_signed_tx(id: u8) -> SignedInjectedTransaction {
        SignedInjectedTransaction::create(
            PrivateKey::random(),
            InjectedTransaction {
                destination: ActorId::zero(),
                payload: vec![id].try_into().unwrap(),
                value: 0,
                reference_block: H256::from_low_u64_be(id as u64),
                salt: vec![id, id].try_into().unwrap(),
            },
        )
        .expect("signing transaction should succeed")
    }

    fn make_announce_tx(signed_tx: &SignedInjectedTransaction) -> AnnounceInjectedTransaction {
        AnnounceInjectedTransaction::from_signed_tx(signed_tx)
    }

    #[test]
    fn announce_from_network_announce_preserves_hashes_and_order() {
        let tx1 = make_signed_tx(1);
        let tx2 = make_signed_tx(2);

        let network_announce = NetworkAnnounce {
            block_hash: H256::from_low_u64_be(42),
            parent: HashOf::random(),
            gas_allowance: Some(123),
            injected_transactions: vec![tx1.clone(), tx2.clone()],
        };

        let from_ref: Announce = (&network_announce).into();
        let from_owned: Announce = network_announce.clone().into();

        assert_eq!(
            from_ref.injected_transactions,
            vec![make_announce_tx(&tx1), make_announce_tx(&tx2)]
        );
        assert_eq!(from_owned, from_ref);
    }

    #[test]
    fn network_announce_try_from_announce_accepts_matching_transactions() {
        let tx1 = make_signed_tx(1);
        let tx2 = make_signed_tx(2);

        let announce = Announce {
            block_hash: H256::from_low_u64_be(10),
            parent: HashOf::random(),
            gas_allowance: Some(999),
            injected_transactions: vec![make_announce_tx(&tx1), make_announce_tx(&tx2)],
        };

        let network_announce =
            NetworkAnnounce::try_from_announce(announce.clone(), vec![tx1.clone(), tx2.clone()])
                .expect("matching announce and transactions should convert");

        assert_eq!(network_announce.block_hash, announce.block_hash);
        assert_eq!(network_announce.parent, announce.parent);
        assert_eq!(network_announce.gas_allowance, announce.gas_allowance);
        assert_eq!(network_announce.injected_transactions, vec![tx1, tx2]);
        assert_eq!(network_announce.to_hash(), announce.to_hash());
    }

    #[test]
    fn network_announce_try_from_announce_rejects_len_mismatch() {
        let tx = make_signed_tx(1);
        let announce = Announce {
            block_hash: H256::from_low_u64_be(7),
            parent: HashOf::random(),
            gas_allowance: None,
            injected_transactions: vec![make_announce_tx(&tx)],
        };

        let error = NetworkAnnounce::try_from_announce(announce, vec![]).unwrap_err();
        assert_eq!(
            error,
            NetworkAnnounceFromAnnounceError::InjectedTransactionsLenMismatch {
                announce_len: 1,
                provided_len: 0,
            }
        );
    }

    #[test]
    fn network_announce_try_from_announce_rejects_hash_mismatch() {
        let tx1 = make_signed_tx(1);
        let tx2 = make_signed_tx(2);
        let announce = Announce {
            block_hash: H256::from_low_u64_be(8),
            parent: HashOf::random(),
            gas_allowance: None,
            injected_transactions: vec![make_announce_tx(&tx1)],
        };

        let error = NetworkAnnounce::try_from_announce(announce, vec![tx2.clone()]).unwrap_err();
        assert_eq!(
            error,
            NetworkAnnounceFromAnnounceError::InjectedTransactionHashMismatch {
                index: 0,
                expected: tx1.data().to_hash(),
                actual: tx2.data().to_hash(),
            }
        );
    }

    #[derive(Default)]
    struct MockInjectedDb(RefCell<Vec<SignedInjectedTransaction>>);

    impl InjectedStorageRO for MockInjectedDb {
        fn injected_transaction(
            &self,
            hash: HashOf<InjectedTransaction>,
        ) -> Option<SignedInjectedTransaction> {
            self.0
                .borrow()
                .iter()
                .find(|tx| tx.data().to_hash() == hash)
                .cloned()
        }
    }

    impl InjectedStorageRW for MockInjectedDb {
        fn set_injected_transaction(&self, tx: SignedInjectedTransaction) {
            self.0.borrow_mut().push(tx);
        }
    }
    // The possible future announce structure
    #[derive(Encode)]
    struct AnnounceV2 {
        block_hash: H256,
        parent: H256,
        gas_allowance: Option<u64>,
        injected_txs_hash: Option<H256>,
    }

    impl AnnounceV2 {
        fn to_hash(&self) -> H256 {
            H256(utils::hash(&self.encode()))
        }
    }

    #[test]
    fn into_announce_persisting_injected_transactions_stores_transactions_and_hashes() {
        let tx1 = make_signed_tx(1);
        let tx2 = make_signed_tx(2);
        let db = MockInjectedDb::default();

        let announce = NetworkAnnounce {
            block_hash: H256::from_low_u64_be(123),
            parent: HashOf::random(),
            gas_allowance: Some(777),
            injected_transactions: vec![tx1.clone(), tx2.clone()],
        }
        .into_announce_persisting_injected_transactions(&db);

        assert_eq!(
            announce.injected_transactions,
            vec![make_announce_tx(&tx1), make_announce_tx(&tx2)]
        );
        assert_eq!(db.0.into_inner(), vec![tx1, tx2]);
    }

    #[test]
    fn test_announce_hash_no_injected() {
        let announce = Announce {
            block_hash: H256::random(),
            parent: unsafe { HashOf::new(H256::random()) },
            gas_allowance: Some(1_000_000),
            injected_transactions: vec![],
        };

        let hash1 = announce.to_hash();
        let hash2 = gear_core::utils::hash(&announce.encode());
        assert_eq!(
            hash1.inner().0,
            hash2,
            "Announce without injected transactions should have the same hash as its SCALE encoding"
        );

        let announce_v2 = AnnounceV2 {
            block_hash: announce.block_hash,
            parent: announce.parent.inner(),
            gas_allowance: announce.gas_allowance,
            injected_txs_hash: None,
        };
        let hash3 = announce_v2.to_hash();
        assert_eq!(
            hash1.inner().0,
            hash3.0,
            "Announce without injected transactions should have the same hash as its possible future announce structure"
        );
    }

    #[test]
    fn test_announce_hash_with_injected() {
        let tx = make_signed_tx(2);
        let announce = Announce {
            block_hash: H256::random(),
            parent: unsafe { HashOf::new(H256::random()) },
            gas_allowance: Some(1_000_000),
            injected_transactions: vec![make_announce_tx(&tx)],
        };
        let hash1 = announce.to_hash();
        let hash2 = gear_core::utils::hash(&announce.encode());
        assert_ne!(
            hash1.inner().0,
            hash2,
            "Announce with injected transactions should have a different hash than its SCALE encoding, unfortunately ..."
        );

        // Just to be sure that hash is calculated from all fields of Announce
        let Announce {
            block_hash,
            parent,
            gas_allowance,
            injected_transactions,
        } = announce.clone();
        let txs_hashes = injected_transactions
            .into_iter()
            .map(|tx| tx.into_parts())
            .collect::<Vec<_>>();
        let maybe_txs_hash = txs_hashes
            .is_empty()
            .not()
            .then(|| utils::hash(&txs_hashes.encode()));
        let announce_parts = (block_hash, parent, gas_allowance, maybe_txs_hash);
        let hash3 = H256(utils::hash(&announce_parts.encode()));
        assert_eq!(
            hash1.inner().0,
            hash3.0,
            "Announce hash should be calculated from all fields of Announce"
        );

        let announce_v2 = AnnounceV2 {
            block_hash: announce.block_hash,
            parent: announce.parent.inner(),
            gas_allowance: announce.gas_allowance,
            injected_txs_hash: maybe_txs_hash.map(H256),
        };

        assert_eq!(
            hash1.inner().0,
            announce_v2.to_hash().0,
            "Announce hash should be consistent with the possible future announce structure"
        );
    }
}
