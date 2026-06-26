// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! In-memory collection of threshold-decryption shares.

use ethexe_common::{Address, HashOf, injected::ShieldedTransaction};
use gprimitives::H256;
use gsigner::{DecryptionShare, PublicDecryptionContext};
use std::{collections::HashMap, sync::Mutex};
use tokio::sync::Notify;

type ShieldedTxHash = HashOf<ShieldedTransaction>;

/// Result of inserting one verified decryption share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertOutcome {
    Inserted,
    Duplicate,
    Equivocation,
    InvalidShare,
    UnknownBlock,
    UnknownTransaction,
}

/// Decryption shares grouped by MB, shielded transaction, and validator.
pub(crate) struct DecryptionSharesStore {
    inner: Mutex<HashMap<H256, BlockShares>>,
    changed: Notify,
}

type BlockShares = HashMap<ShieldedTxHash, HashMap<Address, DecryptionShare>>;

impl DecryptionSharesStore {
    /// Constructs new empty decryption shares store.
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        }
    }

    /// Register the shielded transactions belonging to an assembled MB.
    pub(crate) fn register_block(
        &self,
        mb_hash: H256,
        tx_hashes: impl IntoIterator<Item = ShieldedTxHash>,
    ) {
        let transactions = tx_hashes
            .into_iter()
            .map(|tx_hash| (tx_hash, HashMap::new()))
            .collect();
        self.inner
            .lock()
            .expect("decryption shares poisoned")
            .entry(mb_hash)
            .or_insert(transactions);
    }

    /// Insert a share after checking transaction membership and cryptographic proof.
    pub(crate) fn insert(
        &self,
        mb_hash: H256,
        tx_hash: ShieldedTxHash,
        validator: Address,
        validator_context: &PublicDecryptionContext,
        transaction: &ShieldedTransaction,
        share: DecryptionShare,
    ) -> InsertOutcome {
        if !share.verify(
            &validator_context.blinded_key_share.blinded_key_share,
            &validator_context.validator_public_key.encryption_key,
            &transaction.ciphertext,
        ) {
            return InsertOutcome::InvalidShare;
        }

        let mut blocks = self.inner.lock().expect("decryption shares poisoned");
        let Some(block) = blocks.get_mut(&mb_hash) else {
            return InsertOutcome::UnknownBlock;
        };
        let Some(shares) = block.get_mut(&tx_hash) else {
            return InsertOutcome::UnknownTransaction;
        };
        let outcome = match shares.get(&validator) {
            Some(existing) if existing == &share => InsertOutcome::Duplicate,
            Some(_) => InsertOutcome::Equivocation,
            None => {
                shares.insert(validator, share);
                InsertOutcome::Inserted
            }
        };
        drop(blocks);

        if outcome == InsertOutcome::Inserted {
            self.changed.notify_one();
        }
        outcome
    }

    /// Return exactly `threshold` verified shares ordered by validator address.
    ///
    /// Returns `None` until enough distinct validators have provided a share.
    pub(crate) fn threshold_shares(
        &self,
        mb_hash: H256,
        tx_hash: ShieldedTxHash,
        threshold: usize,
    ) -> Option<Vec<(Address, DecryptionShare)>> {
        let blocks = self.inner.lock().expect("decryption shares poisoned");
        let shares = blocks.get(&mb_hash)?.get(&tx_hash)?;
        if shares.len() < threshold {
            return None;
        }

        let mut validators = shares.keys().copied().collect::<Vec<_>>();
        validators.sort_unstable();
        Some(
            validators
                .into_iter()
                .take(threshold)
                .map(|validator| {
                    let share = shares
                        .get(&validator)
                        .expect("validator was collected from this map")
                        .clone();
                    (validator, share)
                })
                .collect(),
        )
    }

    /// Keep decryption shares only for the finalized MB.
    /// Other shares are no longer useful.
    pub(crate) fn retain_block(&self, mb_hash: H256) {
        let mut this = self.inner.lock().expect("decryption shares poisoned");
        this.retain(|stored_hash, _| *stored_hash == mb_hash);
    }

    pub(crate) fn notified(&self) -> impl Future<Output = ()> + '_ {
        self.changed.notified()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::injected::InjectedTransaction;
    use gear_tdec::{bls12_381::E, rand_utils::Rng};
    use gprimitives::ActorId;

    struct ShareFixture {
        transaction: ShieldedTransaction,
        tx_hash: ShieldedTxHash,
        first_context: PublicDecryptionContext,
        second_context: PublicDecryptionContext,
        first_share: DecryptionShare,
        second_share: DecryptionShare,
    }

    fn share_fixture() -> ShareFixture {
        let mut rng = gear_tdec::rand_utils::test_rng();
        let dealer = gear_tdec::deal::<E>(3, 2, &mut rng);
        let transaction = InjectedTransaction {
            destination: ActorId::from([1; 32]),
            payload: rng.r#gen::<[u8; 32]>().to_vec().try_into().unwrap(),
            value: 0,
            reference_block: H256::random(),
            salt: rng.r#gen::<[u8; 32]>().to_vec().try_into().unwrap(),
        }
        .shield(&dealer.public_key, &mut rng)
        .expect("shielding succeeds");
        let tx_hash = transaction.to_hash();
        let header = transaction.ciphertext.header();
        let aad = transaction.aad.as_ref();
        let first_share = dealer.private_contexts[0]
            .create_share(&header, aad)
            .expect("share creation succeeds");
        let second_share = dealer.private_contexts[1]
            .create_share(&header, aad)
            .expect("share creation succeeds");

        ShareFixture {
            transaction,
            tx_hash,
            first_context: dealer.private_contexts[0].public_decryption_contexts[0].clone(),
            second_context: dealer.private_contexts[1].public_decryption_contexts[1].clone(),
            first_share,
            second_share,
        }
    }

    fn random_tx_hash() -> ShieldedTxHash {
        unsafe { HashOf::new(H256::random()) }
    }

    fn validator(byte: u8) -> Address {
        [byte; 20].into()
    }

    #[tokio::test]
    async fn insertion_is_idempotent_and_notifies() {
        let store = DecryptionSharesStore::new();
        let mb_hash = H256::random();
        let fixture = share_fixture();
        store.register_block(mb_hash, [fixture.tx_hash]);

        assert_eq!(
            store.insert(
                mb_hash,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share.clone()
            ),
            InsertOutcome::Inserted
        );
        tokio::time::timeout(std::time::Duration::from_millis(10), store.notified())
            .await
            .expect("insert notification is retained");
        assert_eq!(
            store.insert(
                mb_hash,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share
            ),
            InsertOutcome::Duplicate
        );
        assert_eq!(
            store
                .threshold_shares(mb_hash, fixture.tx_hash, 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn threshold_query_is_deterministic_and_limited() {
        let store = DecryptionSharesStore::new();
        let mb_hash = H256::random();
        let fixture = share_fixture();
        store.register_block(mb_hash, [fixture.tx_hash]);
        store.insert(
            mb_hash,
            fixture.tx_hash,
            validator(2),
            &fixture.second_context,
            &fixture.transaction,
            fixture.second_share,
        );

        assert!(
            store
                .threshold_shares(mb_hash, fixture.tx_hash, 2)
                .is_none()
        );

        store.insert(
            mb_hash,
            fixture.tx_hash,
            validator(1),
            &fixture.first_context,
            &fixture.transaction,
            fixture.first_share,
        );
        let shares = store
            .threshold_shares(mb_hash, fixture.tx_hash, 1)
            .expect("threshold reached");
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].0, validator(1));
    }

    #[test]
    fn rejects_unknown_entries_and_invalid_shares() {
        let store = DecryptionSharesStore::new();
        let mb_hash = H256::random();
        let fixture = share_fixture();
        let other_tx_hash = random_tx_hash();

        assert_eq!(
            store.insert(
                mb_hash,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share.clone()
            ),
            InsertOutcome::UnknownBlock
        );
        store.register_block(mb_hash, [fixture.tx_hash]);
        assert_eq!(
            store.insert(
                mb_hash,
                other_tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share.clone()
            ),
            InsertOutcome::UnknownTransaction
        );
        assert_eq!(
            store.insert(
                mb_hash,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(
            store.insert(
                mb_hash,
                fixture.tx_hash,
                validator(2),
                &fixture.first_context,
                &fixture.transaction,
                fixture.second_share
            ),
            InsertOutcome::InvalidShare
        );
        assert!(
            store
                .threshold_shares(mb_hash, fixture.tx_hash, 2)
                .is_none()
        );
        assert_eq!(
            store
                .threshold_shares(mb_hash, fixture.tx_hash, 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn finalization_prunes_sibling_blocks() {
        let store = DecryptionSharesStore::new();
        let finalized = H256::random();
        let sibling = H256::random();
        let fixture = share_fixture();
        store.register_block(finalized, [fixture.tx_hash]);
        store.register_block(sibling, [fixture.tx_hash]);
        assert_eq!(
            store.insert(
                sibling,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share.clone()
            ),
            InsertOutcome::Inserted
        );

        store.retain_block(finalized);

        assert_eq!(
            store.insert(
                sibling,
                fixture.tx_hash,
                validator(1),
                &fixture.first_context,
                &fixture.transaction,
                fixture.first_share
            ),
            InsertOutcome::UnknownBlock
        );
    }
}
