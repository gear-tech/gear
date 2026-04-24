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
/// Walks back `canonical_quarantine` steps along `parent_hash`. If the
/// walk would cross `genesis_block_hash` — meaning our chain is too
/// short — returns `Ok(None)`: the producer then emits the next MB
/// **without** an `AdvanceTillEthereumBlock` transaction instead of
/// pretending to anchor on genesis.
///
/// Returns `Err` when a block header is unexpectedly missing from the
/// database (chain-integrity issue); callers treat that as "do not
/// propose yet".
pub fn anchor(
    db: &Database,
    head: SimpleBlockData,
    canonical_quarantine: u8,
    genesis_block_hash: H256,
) -> Result<Option<H256>> {
    let mut current = head.hash;
    let mut header = head.header;

    for _ in 0..canonical_quarantine {
        if current == genesis_block_hash {
            // Too close to genesis — no EB has passed quarantine yet.
            return Ok(None);
        }
        let parent = header.parent_hash;
        header = db
            .block_header(parent)
            .ok_or_else(|| anyhow!("quarantine anchor: missing parent header for {parent}"))?;
        current = parent;
    }

    // If after the full walk we landed exactly on genesis — still not
    // enough depth *past* it (the ethexe quarantine window is defined
    // as "≥ N canonical descendants", genesis has no ancestors to
    // reach).
    if current == genesis_block_hash {
        return Ok(None);
    }

    Ok(Some(current))
}

/// Verify that `candidate` has passed quarantine relative to `head`.
///
/// Concretely: `candidate` must be a canonical ancestor of `head`
/// reached in ≥ `canonical_quarantine` parent steps.
///
/// Returns `Err` when:
/// - `candidate` is not an ancestor within the lookback window (either
///   on a different fork, or our local view is behind the producer —
///   we can't verify in either case, caller should reject the
///   proposal);
/// - `candidate` is an ancestor but the distance is smaller than
///   `canonical_quarantine` (still in quarantine);
/// - a parent header is missing in the DB (chain integrity issue).
///
/// There is no "genesis is always OK" special case here: the producer
/// rule in [`anchor`] is to skip `AdvanceTillEthereumBlock` entirely
/// when the chain is too short, so a validator should never see a
/// proposal anchoring on genesis — and if it does, the walk will
/// correctly fail with "still within quarantine".
pub fn verify_passed(
    db: &Database,
    head: SimpleBlockData,
    candidate: H256,
    canonical_quarantine: u8,
    genesis_block_hash: H256,
) -> Result<()> {
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

        if current == genesis_block_hash {
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
