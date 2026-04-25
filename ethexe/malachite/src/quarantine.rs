// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Canonical-quarantine helpers for the Malachite producer and validators.
//!
//! Both sides operate on the same inputs:
//! - `head`: the most recent Ethereum block each node received via the
//!   observer event stream — **not** `DBGlobals::latest_synced_block`,
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
//! - *"quarantine-passed"* means the block has ≥
//!   [`ComputeConfig::canonical_quarantine`] canonical descendants on top.
//!
//! The walk semantics mirror
//! [`ethexe_compute::utils::find_canonical_events_post_quarantine`] so
//! that the EB the producer anchors to here is exactly the EB whose
//! events the execution layer applies.
//!
//! [`ComputeConfig::canonical_quarantine`]: ethexe_compute::ComputeConfig

use anyhow::{Result, anyhow};
use ethexe_common::{SimpleBlockData, db::OnChainStorageRO};
use ethexe_db::Database;
use gprimitives::H256;

/// Hard cap on how far back of our own chain head we're willing to walk
/// when verifying a peer's `AdvanceTillEthereumBlock`. 1024 ≫ any
/// realistic `canonical_quarantine` (currently 16); prevents a
/// malformed proposal from pinning us on a long DB walk.
const VERIFY_LOOKBACK_SLACK: u32 = 1024;

/// Return the youngest EB that has passed quarantine relative to `head`.
///
/// Walks back `canonical_quarantine` steps along `parent_hash`. Two
/// early-stop conditions:
/// - if the walk reaches `start_block_hash` before finishing — the
///   local chain is too short to clear the quarantine window, return
///   `Ok(None)` so the producer skips `AdvanceTillEthereumBlock`;
/// - if a parent header is unexpectedly missing before we reach the
///   fence — treat as a chain-integrity issue and return `Err`.
pub fn anchor(
    db: &Database,
    head: SimpleBlockData,
    canonical_quarantine: u8,
    start_block_hash: H256,
) -> Result<Option<H256>> {
    let mut current = head.hash;
    let mut header = head.header;

    for _ in 0..canonical_quarantine {
        if current == start_block_hash {
            // We're already on the oldest block the DB knows about —
            // can't take another parent step.
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

/// Verify that `candidate` has passed quarantine relative to `head`.
///
/// Concretely: `candidate` must be a canonical ancestor of `head`
/// reached in ≥ `canonical_quarantine` parent steps. Walks are also
/// capped by `VERIFY_LOOKBACK_SLACK` and stop at `start_block_hash`.
///
/// Returns `Err` when:
/// - candidate is not an ancestor within the lookback window or
///   before we hit the start fence — we can't verify locally;
/// - candidate is an ancestor but at depth `< canonical_quarantine`
///   (still within quarantine);
/// - a parent header is missing from the DB before the fence —
///   chain-integrity issue.
///
/// Dropping a vote because our local view doesn't cover the proposed
/// anchor is an acceptable outcome — the proposal may still reach
/// quorum from validators whose DBs do cover it.
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

/// Whether `candidate` is a *strict* descendant of `ancestor` along
/// the canonical `parent_hash` chain — i.e., `ancestor` appears in
/// `candidate`'s ancestry at depth ≥ 1.
///
/// Cases:
/// - `ancestor == H256::zero()` — pre-genesis sentinel: every block
///   is treated as a descendant. Returns `Ok(true)` immediately.
/// - `ancestor == candidate` — same block, not strict. Returns `Ok(false)`.
/// - walk from `candidate` reaches `ancestor` at depth ≥ 1 →
///   `Ok(true)`.
/// - walk hits genesis (`parent_hash == 0`) before finding `ancestor` →
///   `Err` (orphan: `ancestor` is not in `candidate`'s ancestry —
///   typically means a deep reorg dropped `ancestor` off the
///   canonical chain).
/// - walk hits `start_block_hash` fence before finding `ancestor` →
///   `Err` (local DB doesn't go far enough back to verify).
/// - missing parent header in DB before either of those terminations →
///   `Err` (chain-integrity issue).
///
/// Used by the producer to confirm that a freshly quarantine-passed
/// EB is a proper successor of the parent MB's `last_advanced_block`,
/// not the same block (no progress) and not a sibling on a discarded
/// branch.
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

    /// Persist a synthetic linear chain into the DB and return the
    /// hashes oldest-first. genesis -> blocks[0] (parent = zero) ->
    /// blocks[1] (parent = blocks[0]) -> ...
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
}
