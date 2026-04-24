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
//! Both sides read directly from the shared [`Database`]: the producer
//! to pick the youngest EB that has passed quarantine relative to the
//! local chain head, validators to verify that the producer's pick is
//! a canonical ancestor of the local chain head at depth ≥ quarantine.
//!
//! Convention used across ethexe:
//! - `EB` = Ethereum block;
//! - `MB` = Malachite sequencer block;
//! - *"quarantine-passed"* means the block has at least
//!   [`ComputeConfig::canonical_quarantine`] canonical descendants on top.
//!
//! The semantics mirror
//! [`ethexe_compute::utils::find_canonical_events_post_quarantine`] so
//! that the block the producer anchors to here is exactly the block
//! whose events the execution layer would apply.
//!
//! [`ComputeConfig::canonical_quarantine`]: ethexe_compute::ComputeConfig

use anyhow::{Result, anyhow};
use ethexe_common::db::{ConfigStorageRO, GlobalsStorageRO, OnChainStorageRO};
use ethexe_db::Database;
use gprimitives::H256;

/// Hard cap on how far back of our own chain head we're willing to walk
/// when verifying a peer's `AdvanceTillEthereumBlock`. 1024 ≫ any
/// realistic `canonical_quarantine` (currently 16); prevents a
/// malformed proposal from pinning us on a long DB walk.
const VERIFY_LOOKBACK_SLACK: u32 = 1024;

/// Return the hash of the youngest Ethereum block that has passed
/// quarantine relative to the current local chain head.
///
/// Walks back `canonical_quarantine` steps along `parent_hash` from
/// [`DBGlobals::latest_synced_block`]. If the walk reaches the ethexe
/// genesis block first (chain too short), returns the genesis hash —
/// the producer will emit it as an `AdvanceTillEthereumBlock` value
/// and the executor treats it as "no advance beyond genesis".
///
/// Returns an error only if a block header is unexpectedly missing
/// from the database; callers should treat that as "do not propose
/// yet".
///
/// [`DBGlobals::latest_synced_block`]: ethexe_common::db::DBGlobals
pub fn anchor(db: &Database, canonical_quarantine: u8) -> Result<H256> {
    let head = db.globals().latest_synced_block;
    let genesis = db.config().genesis_block_hash;

    let mut current = head.hash;
    let mut header = head.header;

    for _ in 0..canonical_quarantine {
        if current == genesis {
            return Ok(current);
        }
        let parent = header.parent_hash;
        header = db
            .block_header(parent)
            .ok_or_else(|| anyhow!("quarantine anchor: missing parent header for {parent}"))?;
        current = parent;
    }

    Ok(current)
}

/// Verify that `candidate` has already passed quarantine relative to
/// the local chain head.
///
/// Concretely: `candidate` must be a canonical ancestor of
/// [`DBGlobals::latest_synced_block`] reached in ≥ `canonical_quarantine`
/// parent steps. `candidate == genesis` is always accepted (it cannot
/// go any earlier than genesis).
///
/// Returns `Err` when:
/// - candidate is not an ancestor within the lookback window (either
///   it's on a different fork, or our local view is behind the
///   producer — we can't verify in either case, caller should reject
///   the proposal);
/// - candidate is an ancestor but the distance is smaller than
///   `canonical_quarantine` (still in quarantine);
/// - a parent header is missing in the DB (chain integrity issue).
///
/// [`DBGlobals::latest_synced_block`]: ethexe_common::db::DBGlobals
pub fn verify_passed(db: &Database, candidate: H256, canonical_quarantine: u8) -> Result<()> {
    let head = db.globals().latest_synced_block;
    let genesis = db.config().genesis_block_hash;

    // Producer convention: when our chain is too short to reach
    // `canonical_quarantine`, `anchor` returns genesis. Matching that,
    // we always accept genesis.
    if candidate == genesis {
        return Ok(());
    }

    let canonical_quarantine = canonical_quarantine as u32;
    let max_steps = canonical_quarantine + VERIFY_LOOKBACK_SLACK;

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

        if current == genesis {
            return Err(anyhow!(
                "EB {candidate} is not a canonical ancestor of local chain head \
                 (walk reached genesis at depth {depth})"
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
