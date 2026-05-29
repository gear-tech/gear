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
use ethexe_common::{SimpleBlockData, db::OnChainStorageRO};
use ethexe_db::Database;
use gprimitives::H256;

/// Cap on parent walks when verifying a peer's `AdvanceTillEthereumBlock`.
const VERIFY_LOOKBACK_SLACK: u32 = 100_000;

/// Youngest EB that has cleared quarantine. `Ok(None)` if the local chain is
/// too short, `Err` on missing parent header.
///
/// `depth` is taken as `u32` to accommodate the proposer-side
/// `canonical_quarantine + post_quarantine_delay` sum, which can exceed
/// `u8::MAX`. `verify_passed` still takes `u8` because the protocol
/// invariant validators enforce is anchored on `canonical_quarantine`
/// only — the extra slack is a proposer-side hint.
pub fn anchor(
    db: &Database,
    head: SimpleBlockData,
    depth: u32,
    start_block_hash: H256,
) -> Result<Option<H256>> {
    let mut current = head.hash;
    let mut header = head.header;

    for _ in 0..depth {
        if current == start_block_hash {
            return Ok(None);
        }
        let parent = header.parent_hash;
        header = db
            .block_header(parent)
            .ok_or_else(|| anyhow!("quarantine anchor: missing parent header for {parent}"))?;
        current = parent;
    }

    Ok(Some(current))
}

/// `candidate` must be a canonical ancestor of `head` at depth ≥ `canonical_quarantine`.
/// `Err` on still-quarantined / not-found / missing-parent.
pub fn verify_passed(
    db: &Database,
    head: SimpleBlockData,
    candidate: H256,
    canonical_quarantine: u8,
    start_block_hash: H256,
) -> Result<()> {
    let canonical_quarantine = canonical_quarantine as u32;
    let max_steps = canonical_quarantine.saturating_add(VERIFY_LOOKBACK_SLACK);

    let mut current = head.hash;
    let mut header = head.header;

    for depth in 0..=max_steps {
        if current == candidate {
            return if depth >= canonical_quarantine {
                Ok(())
            } else {
                Err(anyhow!(
                    "EB {candidate} is only {depth} block(s) behind head, \
                     needs ≥ {canonical_quarantine}"
                ))
            };
        }

        if current == start_block_hash {
            return Err(anyhow!(
                "EB {candidate} is not a canonical ancestor of local chain head \
                 (walk reached start_block at depth {depth})"
            ));
        }

        let parent = header.parent_hash;
        header = db.block_header(parent).ok_or_else(|| {
            anyhow!("quarantine verify: missing parent header for {parent} at depth {depth}")
        })?;
        current = parent;
    }

    Err(anyhow!(
        "EB {candidate} not found within {max_steps} ancestors of local chain head"
    ))
}

/// `candidate` strictly descends from `ancestor` (depth ≥ 1). `H256::zero()` =
/// pre-genesis sentinel; equal hashes return `Ok(false)`.
pub fn is_strict_descendant_of(
    db: &Database,
    candidate: H256,
    ancestor: H256,
    start_block_hash: H256,
) -> Result<bool> {
    if ancestor.is_zero() {
        return Ok(true);
    }
    if candidate == ancestor {
        return Ok(false);
    }

    let max_steps = VERIFY_LOOKBACK_SLACK;
    let mut current = candidate;
    let mut header = db
        .block_header(current)
        .ok_or_else(|| anyhow!("descendant check: missing header for candidate {candidate}"))?;

    for _ in 0..max_steps {
        let parent = header.parent_hash;
        if parent == ancestor {
            return Ok(true);
        }
        if parent == H256::zero() {
            return Err(anyhow!(
                "descendant check: ancestor {ancestor} not in canonical ancestry of \
                 candidate {candidate} — walk reached genesis"
            ));
        }
        if current == start_block_hash {
            return Err(anyhow!(
                "descendant check: ancestor {ancestor} not found before start_block fence \
                 starting from candidate {candidate}"
            ));
        }
        header = db
            .block_header(parent)
            .ok_or_else(|| anyhow!("descendant check: missing parent header for {parent}"))?;
        current = parent;
    }

    Err(anyhow!(
        "descendant check: ancestor {ancestor} not found within {max_steps} ancestors \
         of candidate {candidate}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        BlockHeader,
        db::{BlockMetaStorageRW, OnChainStorageRW},
    };

    /// Synthetic linear chain, oldest-first; parent[0] == zero.
    fn linear_chain(db: &Database, len: usize) -> Vec<H256> {
        let mut hashes = Vec::with_capacity(len);
        let mut parent = H256::zero();
        for i in 0..len {
            let mut hash_bytes = [0u8; 32];
            // bias high bytes so each hash is distinct and non-zero.
            hash_bytes[0] = 0xA0 + (i as u8 % 0x60);
            hash_bytes[1] = (i >> 8) as u8;
            hash_bytes[2] = i as u8;
            let hash = H256::from(hash_bytes);
            db.set_block_header(
                hash,
                BlockHeader {
                    height: i as u32,
                    timestamp: i as u64,
                    parent_hash: parent,
                },
            );
            db.mutate_block_meta(hash, |_| {});
            hashes.push(hash);
            parent = hash;
        }
        hashes
    }

    #[test]
    fn zero_ancestor_is_always_descendant() {
        let db = Database::memory();
        let hashes = linear_chain(&db, 3);
        // arbitrary candidate; ancestor = zero (pre-genesis sentinel)
        assert!(is_strict_descendant_of(&db, hashes[2], H256::zero(), H256::zero()).unwrap());
    }

    #[test]
    fn same_block_is_not_strict_descendant() {
        let db = Database::memory();
        let hashes = linear_chain(&db, 3);
        assert!(!is_strict_descendant_of(&db, hashes[1], hashes[1], hashes[0]).unwrap());
    }

    #[test]
    fn proper_ancestor_resolves_to_true() {
        let db = Database::memory();
        let hashes = linear_chain(&db, 5);
        // hashes[4] should be a strict descendant of hashes[1]
        // through 3 parent steps.
        assert!(is_strict_descendant_of(&db, hashes[4], hashes[1], hashes[0]).unwrap());
    }

    #[test]
    fn unrelated_ancestor_errors() {
        let db = Database::memory();
        let hashes = linear_chain(&db, 5);
        // ancestor = a hash that's not in the chain at all
        let mut orphan_bytes = [0xFFu8; 32];
        orphan_bytes[0] = 0x42;
        let orphan = H256::from(orphan_bytes);
        let res = is_strict_descendant_of(&db, hashes[4], orphan, hashes[0]);
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
        /// (chain_len, q) with `q < chain_len` the returned hash is
        /// the block at index `chain_len - 1 - q`.
        #[test]
        fn anchor_walks_exactly_q_steps(
            chain_len in 2usize..32,
            q in 0u8..16,
        ) {
            let q_usize = q as usize;
            prop_assume!(q_usize < chain_len);
            let db = Database::memory();
            let hashes = linear_chain(&db, chain_len);
            let head = SimpleBlockData {
                hash: hashes[chain_len - 1],
                header: ethexe_common::BlockHeader {
                    height: (chain_len - 1) as u32,
                    timestamp: (chain_len - 1) as u64,
                    parent_hash: if chain_len >= 2 { hashes[chain_len - 2] } else { H256::zero() },
                },
            };
            // start_block = genesis (so the fence never trips).
            let result = anchor(&db, head, q as u32, hashes[0]).unwrap();
            let expected = hashes[chain_len - 1 - q_usize];
            prop_assert_eq!(result, Some(expected));
        }

        /// `is_strict_descendant_of(c, a)` is the transitive closure
        /// of "next-block": for any (i, j) on a single chain, with
        /// `i > j > 0`, the chain[i] descends from chain[j]; with
        /// `i == j`, it does NOT (strictness).
        #[test]
        fn descendant_relation_matches_chain_indices(
            chain_len in 2usize..16,
            i in 1usize..16,
            j in 0usize..16,
        ) {
            prop_assume!(i < chain_len);
            prop_assume!(j < chain_len);
            let db = Database::memory();
            let hashes = linear_chain(&db, chain_len);

            let result = is_strict_descendant_of(&db, hashes[i], hashes[j], hashes[0]);
            if i > j {
                prop_assert_eq!(result.unwrap(), true);
            } else if i == j {
                prop_assert_eq!(result.unwrap(), false);
            } else {
                // i < j → walking back from i never reaches j.
                // The walk hits genesis (parent_hash zero) → Err.
                prop_assert!(result.is_err());
            }
        }

        /// `verify_passed(head, candidate)` succeeds iff `candidate`
        /// sits at depth >= q from `head` on the canonical chain.
        #[test]
        fn verify_passed_matches_depth(
            chain_len in 4usize..16,
            head_idx in 0usize..16,
            cand_idx in 0usize..16,
            q in 0u8..6,
        ) {
            prop_assume!(head_idx < chain_len);
            prop_assume!(cand_idx <= head_idx);
            let db = Database::memory();
            let hashes = linear_chain(&db, chain_len);
            let head_hash = hashes[head_idx];
            let head_height = head_idx as u32;
            let head_parent = if head_idx > 0 { hashes[head_idx - 1] } else { H256::zero() };
            let head = SimpleBlockData {
                hash: head_hash,
                header: ethexe_common::BlockHeader {
                    height: head_height,
                    timestamp: head_idx as u64,
                    parent_hash: head_parent,
                },
            };
            let depth = head_idx - cand_idx;
            let result = verify_passed(&db, head, hashes[cand_idx], q, hashes[0]);
            if depth >= q as usize {
                prop_assert!(result.is_ok(), "expected pass: {result:?}");
            } else {
                prop_assert!(result.is_err(), "expected too-shallow err: {result:?}");
            }
        }
    }
}
