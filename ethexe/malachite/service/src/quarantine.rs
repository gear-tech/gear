// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Canonical-quarantine helpers for the Malachite producer and validators.
//!
//! Both sides operate on the same inputs:
//! - `head`: the most recent Ethereum block each node received via the
//!   observer event stream — **not** `DBGlobals::latest_synced_eb`,
//!   which trails the event stream and is updated only after extra
//!   processing.
//! - the shared [`ethexe_db::Database`] as a source of
//!   `parent_hash` links along the canonical chain.
//! - `start_block_hash` — the **oldest** block the local DB is
//!   guaranteed to have a header for (fast-synced nodes start there,
//!   not at genesis). Walks never cross this fence; if a walk would
//!   have to go past it we conclude the local view is insufficient
//!   and return `Ok(None)` / `Err` accordingly. It's acceptable for
//!   a validator to abstain from voting for a proposal in that case.
//!
//! Convention:
//! - `EB` = Ethereum block;
//! - `MB` = Malachite sequencer block;
//! - *"quarantine-passed"* means the block has ≥ `canonical_quarantine`
//!   canonical descendants on top.

use anyhow::{Result, anyhow};
use ethexe_common::{Acceptance, SimpleBlockData, db::OnChainStorageRO};
use gprimitives::H256;

/// Youngest EB that has cleared the `depth` quarantine against the local head.
/// Returns `Ok(None)` if no blocks passed the depth check since `start_eb`.
///
/// # Caller guarantees
/// - `head` is synced block
/// - `start_eb_hash` is hash of global start block (DB genesis or fast-sync pivot)
pub fn anchor(
    db: &impl OnChainStorageRO,
    head: SimpleBlockData,
    depth: u32,
    start_eb: H256,
) -> Result<Option<SimpleBlockData>> {
    let mut cursor = head;
    for _ in 0..depth {
        if cursor.hash == start_eb {
            return Ok(None);
        }
        cursor = db
            .block_simple_data(cursor.header.parent_hash)
            .ok_or_else(|| anyhow!("quarantine anchor: missing header for {cursor}"))?;
    }

    Ok(Some(cursor))
}

/// Check whether `candidate` strictly descends from `ancestor`
///
/// # Caller guarantees
/// - `candidate` is synced block
/// - `ancestor` is hash of synced block or zero (pre-genesis sentinel)
/// - `start_eb_hash` is hash of global start block (DB genesis or fast-sync pivot)
pub fn is_strict_descendant_of(
    db: &impl OnChainStorageRO,
    candidate: SimpleBlockData,
    ancestor_hash: H256,
    start_eb_hash: H256,
) -> Result<Acceptance<(), String>> {
    if ancestor_hash == H256::zero() {
        // Special case: all blocks descend from the zero hash (pre-genesis sentinel).
        return Ok(Acceptance::Accepted(()));
    }

    let ancestor = db
        .block_simple_data(ancestor_hash)
        .ok_or_else(|| anyhow!("descendant check: missing header for ancestor {ancestor_hash}"))?;

    let Some(depth) = candidate.header.height.checked_sub(ancestor.header.height) else {
        return Ok(Acceptance::Rejected(format!(
            "candidate {candidate} height is not greater than ancestor {ancestor}"
        )));
    };

    let mut cursor = candidate;
    for _ in 0..depth {
        let parent_hash = cursor.header.parent_hash;
        if parent_hash == ancestor.hash {
            // Found the ancestor
            return Ok(Acceptance::Accepted(()));
        }

        if parent_hash == start_eb_hash {
            return Ok(Acceptance::Rejected(format!(
                "ancestor {ancestor} not found within candidate's ancestry (walk reached start EB)"
            )));
        }

        cursor = db
            .block_simple_data(parent_hash)
            .ok_or_else(|| anyhow!("descendant check: missing header for parent {parent_hash}"))?;
    }

    Ok(Acceptance::Rejected(format!(
        "candidate {candidate} does not descend from ancestor {ancestor}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{BlockHeader, db::OnChainStorageRW};
    use ethexe_db::Database;

    /// Synthetic linear chain, oldest-first; parent[0] == zero.
    fn linear_chain(db: &Database, len: usize) -> Vec<SimpleBlockData> {
        let mut blocks = Vec::with_capacity(len);
        let mut parent = H256::zero();
        for i in 0..len {
            let mut hash_bytes = [0u8; 32];
            // bias high bytes so each hash is distinct and non-zero.
            hash_bytes[0] = 0xA0 + (i as u8 % 0x60);
            hash_bytes[1] = (i >> 8) as u8;
            hash_bytes[2] = i as u8;
            let hash = H256::from(hash_bytes);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            blocks.push(SimpleBlockData { hash, header });
            parent = hash;
        }
        blocks
    }

    #[test]
    fn zero_ancestor_is_always_descendant() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        // arbitrary candidate; ancestor = zero (pre-genesis sentinel)
        assert!(
            is_strict_descendant_of(&db, chain[2], H256::zero(), H256::zero())
                .unwrap()
                .is_accepted()
        );
    }

    #[test]
    fn same_block_is_not_strict_descendant() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        assert!(
            is_strict_descendant_of(&db, chain[1], chain[1].hash, chain[0].hash)
                .unwrap()
                .is_rejected()
        );
    }

    #[test]
    fn proper_ancestor_resolves_to_accepted() {
        let db = Database::memory();
        let chain = linear_chain(&db, 5);
        // chain[4] should be a strict descendant of chain[1]
        // through 3 parent steps.
        assert!(
            is_strict_descendant_of(&db, chain[4], chain[1].hash, chain[0].hash)
                .unwrap()
                .is_accepted()
        );
    }

    #[test]
    fn unrelated_ancestor_errors() {
        let db = Database::memory();
        let chain = linear_chain(&db, 5);
        // ancestor = a hash that's not in the chain at all — the local
        // header lookup fails, which is a local-view error, not a vote.
        let mut orphan_bytes = [0xFFu8; 32];
        orphan_bytes[0] = 0x42;
        let orphan = H256::from(orphan_bytes);
        let res = is_strict_descendant_of(&db, chain[4], orphan, chain[0].hash);
        assert!(res.is_err(), "expected Err for orphan ancestor: {res:?}");
    }

    // ----------------------------------------------------------------
    // Property tests
    // ----------------------------------------------------------------

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// `anchor(head)` walks back exactly `canonical_quarantine`
        /// steps along the canonical chain, so for any pair
        /// (chain_len, q) with `q < chain_len` the returned block is
        /// the one at index `chain_len - 1 - q`.
        #[test]
        fn anchor_walks_exactly_q_steps(
            chain_len in 2usize..32,
            q in 0u8..16,
        ) {
            let q_usize = q as usize;
            prop_assume!(q_usize < chain_len);
            let db = Database::memory();
            let chain = linear_chain(&db, chain_len);
            let head = chain[chain_len - 1];
            // start_block = genesis (so the fence never trips).
            let result = anchor(&db, head, q as u32, chain[0].hash).unwrap();
            let expected = chain[chain_len - 1 - q_usize];
            prop_assert_eq!(result, Some(expected));
        }

        /// `is_strict_descendant_of(c, a)` is the transitive closure
        /// of "next-block": for any (i, j) on a single chain, with
        /// `i > j`, chain[i] descends from chain[j]; with `i <= j`,
        /// it does NOT (strictness / height check).
        #[test]
        fn descendant_relation_matches_chain_indices(
            chain_len in 2usize..16,
            i in 1usize..16,
            j in 0usize..16,
        ) {
            prop_assume!(i < chain_len);
            prop_assume!(j < chain_len);
            let db = Database::memory();
            let chain = linear_chain(&db, chain_len);

            let result = is_strict_descendant_of(&db, chain[i], chain[j].hash, chain[0].hash)
                .unwrap();
            if i > j {
                prop_assert!(result.is_accepted(), "expected Accepted: {result:?}");
            } else {
                // i == j → strictness; i < j → candidate height below
                // ancestor height. Both are proposer-controlled inputs,
                // so they reject (vote nil) rather than error.
                prop_assert!(result.is_rejected(), "expected Rejected: {result:?}");
            }
        }
    }
}
