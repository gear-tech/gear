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

//! # Theory of Announce Propagation
//!
//! ## Definitions
//! - `block` - an ethereum block.
//! - `announce` - see [Announce](ethexe_common::Announce).
//! - `announce.for_block` - block for which announce was created.
//! - `announce.committed_at_block` - block where announce was committed (if it was committed).
//! - `announce.branch` - linked chain of announces starting from `start_announce` to `announce` itself.
//! - `base announce` - announce which does not have any injected transactions and gas allowance.
//! - `not-base announce` - any announce which cannot be classified as base announce.
//! - `commitment_delay_limit` - protocol parameter defining maximal delay (in blocks)
//!   for committing announces not-base announces.
//! - `start_block` - genesis block (for ethexe) or defined by fast_sync block,
//!   It's guaranteed that it's predecessor of any new chain head coming from ethereum.
//!   Always has only one announce, which is called `start_announce`.
//! - `block.announces` - set of announces connected to the `block`. All announces in this set
//!   are created for this `block`.
//! - `included announce` - announce which has been included in `block.announces` of `announce.for_block`.
//!   It's guaranteed that if announce is included, than announce body is set in db also.
//! - `block.last_committed_announce` - last committed announce at `block` (can be committed in predecessors).
//! - `propagated block` - block for which announces were propagated. Must have at least one announce in `block.announces`.
//! - `not propagated block` - block for which announces were not propagated yet. Announces must be None in database.
//!
//! ## Statements
//! Statements below correct only if majority ( > 2/3 ) of validators are correct and honest.
//!
//! ### STATEMENT1 (S1)
//! Any not-base `announce` created by producer for some `block` can be committed in `block1` only if
//! 1) `block1` is a strict successor of `block`
//! 2) `block1.height - block.height <= commitment_delay_limit`
//!
//! ### STATEMENT2 (S2)
//! If it's known at `block` that `announce1` has been committed
//! and `announce2` has been committed after `announce1`, then
//! 1) `announce2` is strict successor of `announce1`
//! 2) `announce2.for_block` is a strict successor of `announce1.for_block`
//! 3) `announce2.committed_at_block` is a successor of `announce1.committed_at_block`
//!
//! ### STATEMENT3 (S3)
//! About local announces propagation. For correctness, strict rules must be followed to propagate announces.
//! If we have `block1` and `block2`, where `block2.parent == block1`, then
//! for any announce from `block2.announces` next statements must be true:
//! 1) `block1.announces.contains(announce.parent)`
//! 2) `announce.chain.contains(block2.last_committed_announce)`
//! 3) Any not-base announce1 from `announce.chain` is committed before `commitment_delay_limit`, except
//!    maybe `commitment_delay_limit` newest announces in the `announce.chain`.
//!
//! ## Theorem and Consequences
//!
//! ### Definitions for Theorem 1
//! - `block` - new received block from ethereum network.
//! - `lpb` - last propagated block, i.e. last predecessor of `block` for which announces were propagated.
//! - `chain` - ordered set of not propagated blocks till `block` (inclusive).
//!
//! ### THEOREM 1 (T1)
//! If `announce` is any announce committed in any block from `chain`
//! and `announce` is not yet included by this node,
//! then `common_predecessor_announce` must exists, such that
//! 1) included by this node
//! 2) strict predecessor of `announce`
//! 3) strict predecessor of at least one announce from `lpb.announces`
//! 4) `lpb.height - commitment_delay_limit <= common_predecessor_announce.for_block.height < lpb.height`
//!
//! ### T1 Consequences
//! If `announce` is committed in some block from `chain` and
//! this `announce` is not included yet, then
//! 1) (T1S1) `announce.for_block.height > lpb.height - commitment_delay_limit`
//! 2) (T1S2) if `announce1` is predecessor of any announce from `lpb.announces`
//!    and `announce1.for_block.height <= lpb.height - commitment_delay_limit`,
//!    then `announce1` is strict predecessor of `announce` and is predecessor of each
//!    announce from `lpb.announces`.

use crate::tx_validation::{TxValidity, TxValidityChecker};
use anyhow::{Result, anyhow, ensure};
use ethexe_common::{
    Announce, HashOf, MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE, SimpleBlockData, WithHashOf,
    db::{
        AnnounceStorageRW, BlockMetaStorageRW, GlobalsStorageRO, InjectedStorageRW,
        OnChainStorageRO,
    },
    network::{AnnouncesRequest, AnnouncesRequestUntil},
};
use ethexe_ethereum::primitives::map::HashMap;
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use std::collections::{BTreeSet, VecDeque};

pub trait DBAnnouncesExt:
    AnnounceStorageRW
    + BlockMetaStorageRW
    + OnChainStorageRO
    + GlobalsStorageRO
    + InjectedStorageRW
    + Storage
{
    /// Collects blocks from the chain head backwards till the first propagated block found.
    fn collect_blocks_without_announces(&self, head: H256) -> Result<VecDeque<SimpleBlockData>>;

    /// Include announce into the database and link it to its block.
    /// Returns (announce_hash, is_newly_included).
    /// - `announce_hash` is the hash of the included announce.
    /// - `is_newly_included` is true if the announce was not included before, false otherwise.
    fn include_announce(&self, announce: Announce) -> Result<(HashOf<Announce>, bool)>;

    /// Check whether announce is already included.
    fn is_announce_included(&self, announce_hash: HashOf<Announce>) -> bool;

    /// Get set of parents for the given set of announces.
    fn announces_parents(
        &self,
        announces: impl IntoIterator<Item = HashOf<Announce>>,
    ) -> Result<BTreeSet<HashOf<Announce>>>;

    /// Find block announce satisfying provided predicate.
    fn find_block_announce(
        &self,
        block_hash: H256,
        pred: impl Fn(&WithHashOf<Announce>) -> bool,
    ) -> Result<Option<WithHashOf<Announce>>>;
}

impl<
    DB: AnnounceStorageRW
        + BlockMetaStorageRW
        + OnChainStorageRO
        + GlobalsStorageRO
        + InjectedStorageRW
        + Storage,
> DBAnnouncesExt for DB
{
    fn collect_blocks_without_announces(&self, head: H256) -> Result<VecDeque<SimpleBlockData>> {
        let mut blocks = VecDeque::new();
        let mut current_block = head;
        loop {
            let header = self
                .block_header(current_block)
                .ok_or_else(|| anyhow!("header not found for block({current_block})"))?;

            if self.block_meta(current_block).announces.is_some() {
                break;
            }

            blocks.push_front(SimpleBlockData {
                hash: current_block,
                header,
            });
            current_block = header.parent_hash;
        }

        Ok(blocks)
    }

    fn include_announce(&self, announce: Announce) -> Result<(HashOf<Announce>, bool)> {
        tracing::trace!(announce = %announce.to_hash(), "Including announce...");

        let block_hash = announce.block_hash;
        let announce_hash = self.set_announce(announce);

        let mut newly_included = None;
        self.mutate_block_meta(block_hash, |meta| {
            if let Some(announces) = &mut meta.announces {
                newly_included = Some(announces.insert(announce_hash));
            }
        });

        if let Some(newly_included) = newly_included {
            Ok((announce_hash, newly_included))
        } else {
            Err(anyhow!(
                "Block announces are missing for block({block_hash})"
            ))
        }
    }

    fn is_announce_included(&self, announce_hash: HashOf<Announce>) -> bool {
        // Zero announce hash is always included (it's a parent of the genesis announce)
        if announce_hash == HashOf::zero() {
            return true;
        }

        self.announce(announce_hash)
            .and_then(|announce| self.block_meta(announce.block_hash).announces)
            .map(|announces| announces.contains(&announce_hash))
            .unwrap_or(false)
    }

    fn announces_parents(
        &self,
        announces: impl IntoIterator<Item = HashOf<Announce>>,
    ) -> Result<BTreeSet<HashOf<Announce>>> {
        announces
            .into_iter()
            .map(|announce_hash| {
                self.announce(announce_hash)
                    .map(|a| a.parent)
                    .ok_or_else(|| anyhow!("Announce {announce_hash:?} not found"))
            })
            .collect()
    }

    fn find_block_announce(
        &self,
        block_hash: H256,
        pred: impl Fn(&WithHashOf<Announce>) -> bool,
    ) -> Result<Option<WithHashOf<Announce>>> {
        let announces = self
            .block_meta(block_hash)
            .announces
            .ok_or_else(|| anyhow!("announces not found for block({block_hash})"))?;

        for announce_hash in announces {
            let announce = self
                .announce(announce_hash)
                .ok_or_else(|| anyhow!("announce({announce_hash}) not found"))?;

            let with_hash = WithHashOf {
                hash: announce_hash,
                value: announce,
            };

            if pred(&with_hash) {
                return Ok(Some(with_hash));
            }
        }

        Ok(None)
    }
}

/// Propagate announces along the provided chain of blocks.
/// if some committed in blocks from chain announces are missing,
/// they must be presented in `missing_announces` map.
/// Missing announces will be included in the database
/// during propagation in recovery process, see [`announces_chain_recovery_if_needed`].
/// After successful propagation all blocks in the chain will become propagated.
pub fn propagate_announces(
    db: &impl DBAnnouncesExt,
    chain: VecDeque<SimpleBlockData>,
    commitment_delay_limit: u32,
    mut missing_announces: HashMap<HashOf<Announce>, Announce>,
) -> Result<()> {
    // iterate over the collected blocks from oldest to newest and propagate announces
    for block in chain {
        debug_assert!(
            db.block_meta(block.hash).announces.is_none(),
            "Block {} should not have announces propagated yet",
            block.hash
        );

        let last_committed_announce_hash = db
            .block_meta(block.hash)
            .last_committed_announce
            .ok_or_else(|| {
                anyhow!(
                    "Last committed announce hash not found for prepared block({})",
                    block.hash
                )
            })?;

        recover_announces_chain_if_needed(
            db,
            &block,
            last_committed_announce_hash,
            commitment_delay_limit,
            &mut missing_announces,
        )?;

        let mut new_base_announces = BTreeSet::new();
        for parent_announce_hash in db
            .block_meta(block.header.parent_hash)
            .announces
            .ok_or_else(|| {
                anyhow!(
                    "Parent block({}) announces are missing",
                    block.header.parent_hash
                )
            })?
        {
            if let Some(new_base_announce) = propagate_one_base_announce(
                db,
                block.hash,
                parent_announce_hash,
                last_committed_announce_hash,
                commitment_delay_limit,
            )? {
                let announce_hash = db.set_announce(new_base_announce);
                new_base_announces.insert(announce_hash);
            };
        }

        // If error: DB is corrupted, or statements S1-S3 were violated by validators
        ensure!(
            !new_base_announces.is_empty(),
            "at least one announce must be propagated for block({})",
            block.hash
        );

        db.mutate_block_meta(block.hash, |meta| {
            debug_assert!(
                meta.announces.is_none(),
                "block({}) announces must be None before propagation",
                block.hash
            );
            meta.announces = Some(new_base_announces);
        });
    }

    Ok(())
}

/// Recover announces chain if it was committed but not included yet by this node.
/// For example node has following chain:
/// ```text
/// [B1] <-- [B2] <-- [B3] <-- [B4] <-- [B5]  (blocks)
///  |        |        |        |
/// (A1) <-- (A2) <-- (A3) <-- (A4)  (announces)
/// ```
/// Then node checks events that unknown announce `(A3')` was committed at block `B5`.
/// Then node have to recover the chain of announces to include `(A3')` and its predecessors:
/// ```text
/// [B1] <-- [B2] <-- [B3] <-- [B4] <-- [B5]  (blocks)
///  |        |        |        |
/// (A1) <-- (A2) <-- (A3) <-- (A4)  (announces)
///   \
///     ---- (A2') <- (A3') <- (A4') (recovered announces)
/// ```
/// where `(A3')` and `(A2')` are committed and must be presented in `missing_announces`,
/// and `(A4')` is base announce propagated from `(A3')`.
fn recover_announces_chain_if_needed(
    db: &impl DBAnnouncesExt,
    block: &SimpleBlockData,
    last_committed_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
    missing_announces: &mut HashMap<HashOf<Announce>, Announce>,
) -> Result<()> {
    // TODO: #4941 append recovery from rejected announces
    // if node received announce, which was rejected because of incorrect parent,
    // but later we receive event from ethereum that parent announce was committed,
    // than node should use previously rejected announce to recover the chain.

    // Recover backwards the chain of committed announces till last included one
    // According to T1, this chain must not be longer than commitment_delay_limit
    let mut last_committed_announce_block_hash = None;
    let mut current_announce_hash = last_committed_announce_hash;
    let mut count = 0;
    while count < commitment_delay_limit && !db.is_announce_included(current_announce_hash) {
        tracing::debug!(announce = %current_announce_hash, "Committed announces was not included yet, try to recover...");

        let announce = missing_announces.remove(&current_announce_hash).ok_or_else(|| {
            anyhow!(
                "Committed announce {current_announce_hash} is missing, but not found in missing announces"
            )
        })?;

        last_committed_announce_block_hash.get_or_insert(announce.block_hash);

        current_announce_hash = announce.parent;
        count += 1;

        let (announce_hash, newly_included) = db.include_announce(announce)?;
        debug_assert!(
            newly_included,
            "announce({announce_hash}) must be newly included during recovery",
        );
    }

    let Some(last_committed_announce_block_hash) = last_committed_announce_block_hash else {
        // No committed announces were missing, no need to recover
        return Ok(());
    };

    // If error: DB is corrupted, or incorrect commitment detected (have not-base announce committed after commitment delay limit)
    ensure!(
        db.is_announce_included(current_announce_hash),
        "{current_announce_hash} is not included after checking {commitment_delay_limit} announces",
    );

    // Recover forward the chain filling with base announces

    // First collect a chain of blocks from `last_committed_announce_block_hash` to `block` (exclusive)
    // According to T1, this chain must not be longer than commitment_delay_limit
    let mut current_block_hash = block.header.parent_hash;
    let mut chain = VecDeque::new();
    let mut count = 0;
    while count < commitment_delay_limit && current_block_hash != last_committed_announce_block_hash
    {
        chain.push_front(current_block_hash);
        current_block_hash = db
            .block_header(current_block_hash)
            .ok_or_else(|| anyhow!("header not found for block({current_block_hash})"))?
            .parent_hash;
        count += 1;
    }

    // If error: DB is corrupted, or incorrect commitment detected (have not-base announce committed after commitment delay limit)
    ensure!(
        current_block_hash == last_committed_announce_block_hash,
        "last committed announce block {last_committed_announce_block_hash} not found \
        in parent chain of block {} within {commitment_delay_limit} blocks",
        block.hash
    );

    // Now propagate base announces along the chain
    let mut parent_announce_hash = last_committed_announce_hash;
    for block_hash in chain {
        let new_base_announce = Announce::base(block_hash, parent_announce_hash);
        let (announce_hash, newly_included) = db.include_announce(new_base_announce)?;
        debug_assert!(
            newly_included,
            "announce({announce_hash}) must be newly included during recovery",
        );
        parent_announce_hash = announce_hash;
    }

    Ok(())
}

/// Create a new base announce from provided parent announce hash,
/// if it's not break the rules defined in S3.
fn propagate_one_base_announce(
    db: &impl DBAnnouncesExt,
    block_hash: H256,
    parent_announce_hash: HashOf<Announce>,
    last_committed_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
) -> Result<Option<Announce>> {
    tracing::trace!(
        block = %block_hash,
        parent_announce = %parent_announce_hash,
        last_committed_announce = %last_committed_announce_hash,
        "Trying propagating new base announce from parent announce",
    );

    // Check that parent announce branch is not expired
    // The branch is expired if:
    // 1. It does not includes last committed announce
    // 2. If it includes not committed and not-base announce, which is older than commitment delay limit.
    //
    // We check here till commitment delay limit, because T1 guaranties that enough.
    let mut current_announce_hash = parent_announce_hash;
    for i in 0..commitment_delay_limit {
        if current_announce_hash == last_committed_announce_hash {
            // We found last committed announce in the branch, until commitment delay limit
            // that means this branch is still not expired.
            break;
        }

        let current_announce = db
            .announce(current_announce_hash)
            .ok_or_else(|| anyhow!("announce({current_announce_hash}) not found"))?;

        if i == commitment_delay_limit - 1 && !current_announce.is_base() {
            // We reached the oldest announce in commitment delay limit which is not committed yet.
            // This announce cannot be committed any more if it is not-base announce,
            // so this branch is expired and we have to skip propagation from `parent`.
            tracing::trace!(
                predecessor = %current_announce_hash,
                parent_announce = %parent_announce_hash,
                "predecessor is too old and not-base, so parent announce branch is expired",
            );
            return Ok(None);
        }

        // Check neighbor announces to be last committed announce
        if db
            .block_meta(current_announce.block_hash)
            .announces
            .ok_or_else(|| {
                anyhow!(
                    "announces are missing for block({})",
                    current_announce.block_hash
                )
            })?
            .contains(&last_committed_announce_hash)
        {
            // We found last committed announce in the neighbor branch, until commitment delay limit
            // that means this branch is already expired.
            tracing::trace!(
                predecessor = %current_announce_hash,
                parent_announce = %parent_announce_hash,
                last_committed_announce = %last_committed_announce_hash,
                "neighbor announce branch contains last committed announce, so parent announce branch is expired",
            );
            return Ok(None);
        };

        current_announce_hash = current_announce.parent;
    }

    let new_base_announce = Announce::base(block_hash, parent_announce_hash);

    tracing::trace!(
        parent_announce = %parent_announce_hash,
        new_base_announce = %new_base_announce.to_hash(),
        "branch from parent announce is not expired, propagating new base announce",
    );

    Ok(Some(new_base_announce))
}

/// Check whether there are missing announces to be requested from peers.
/// If there are missing announces, returns announces request to get them.
pub fn check_for_missing_announces(
    db: &impl DBAnnouncesExt,
    head: H256,
    last_with_announces_block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<Option<AnnouncesRequest>> {
    let last_committed_announce_hash = db
        .block_meta(head)
        .last_committed_announce
        .ok_or_else(|| anyhow!("last committed announce not found for block {head}"))?;

    if db.is_announce_included(last_committed_announce_hash) {
        // announce is already included, no need to request announces

        #[cfg(debug_assertions)]
        {
            // debug check that all announces in the chain are present (check only up to 100 announces)
            let start_announce_hash = db.globals().start_announce_hash;

            let start_announce_block_height = db
                .announce(start_announce_hash)
                .and_then(|announce| db.block_header(announce.block_hash))
                .expect("start block data corrupted in db")
                .height;

            let last_committed_announce_block_height =
                if last_committed_announce_hash == HashOf::zero() {
                    0u32
                } else {
                    db.announce(last_committed_announce_hash)
                        .and_then(|announce| db.block_header(announce.block_hash))
                        .expect("last committed announce data corrupted in db")
                        .height
                };

            let mut announce_hash = last_committed_announce_hash;
            let mut count = last_committed_announce_block_height
                .saturating_sub(start_announce_block_height)
                .min(100);
            while count > 0 && announce_hash != start_announce_hash {
                assert!(
                    db.is_announce_included(announce_hash),
                    "announce {announce_hash} must be included"
                );

                announce_hash = db
                    .announce(announce_hash)
                    .unwrap_or_else(|| panic!("announce {announce_hash} not found"))
                    .parent;
                count -= 1;
            }
        }

        Ok(None)
    } else {
        // announce is not included, so there can be missing announces
        // and node needs to request all announces till definitely known one
        let common_predecessor_announce_hash = find_announces_common_predecessor(
            db,
            last_with_announces_block_hash,
            commitment_delay_limit,
        )?;

        Ok(Some(AnnouncesRequest {
            head: last_committed_announce_hash,
            until: AnnouncesRequestUntil::Tail(common_predecessor_announce_hash),
        }))
    }
}

/// Returns hash of announce from T1S2 or start_announce
fn find_announces_common_predecessor(
    db: &impl DBAnnouncesExt,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<HashOf<Announce>> {
    let start_announce_hash = db.globals().start_announce_hash;

    let mut announces = db
        .block_meta(block_hash)
        .announces
        .ok_or_else(|| anyhow!("announces not found for block {block_hash}"))?;

    for _ in 0..commitment_delay_limit {
        if announces.contains(&start_announce_hash) {
            if announces.len() != 1 {
                return Err(anyhow!(
                    "Start announce {start_announce_hash} reached, but multiple announces present"
                ));
            }
            return Ok(start_announce_hash);
        }

        announces = db.announces_parents(announces)?;
    }

    if let Some(announce) = announces.iter().next()
        && announces.len() == 1
    {
        Ok(*announce)
    } else {
        // common predecessor not found by some reasons
        // This can happen for example, if some old not-base announce was committed
        // and T1S2 cannot be applied.
        Err(anyhow!(
            "Common predecessor for announces in block {block_hash} in nearest {commitment_delay_limit} blocks not found",
        ))
    }
}

/// Returns announce hash, which is supposed to be best
/// to produce a new announce above at `block_hash`.
/// Used to produce new announce or validate announce from producer.
pub fn best_parent_announce(
    db: &impl DBAnnouncesExt,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<HashOf<Announce>> {
    let announces = db
        .block_meta(block_hash)
        .announces
        .ok_or_else(|| anyhow!("announces not found for block {block_hash}"))?;

    // We do not take announces directly from parent block,
    // because some of them may be expired at `block_hash`,
    // so we take parents of all announces from `block_hash`,
    // to be sure that we take only not expired parent announces.
    let candidates = db.announces_parents(announces)?;

    best_announce(
        db,
        candidates,
        commitment_delay_limit
            .checked_sub(1)
            .expect("commitment_delay_limit must be > 0"),
    )
}

/// Returns best announce for `block_hash`.
pub fn block_best_announce(
    db: &impl DBAnnouncesExt,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<HashOf<Announce>> {
    let best_parent = best_parent_announce(db, block_hash, commitment_delay_limit)?;

    let not_base_announce_hash = db.find_block_announce(block_hash, |announce| {
        announce.value.parent == best_parent && !announce.value.is_base()
    })?;
    let base_announce_hash = db.find_block_announce(block_hash, |announce| {
        announce.value.parent == best_parent && announce.value.is_base()
    })?;

    match (not_base_announce_hash, base_announce_hash) {
        (Some(not_base), Some(base)) => {
            if announces_have_equal_outcomes(db, base.hash, not_base.hash) {
                // if base announce has the same outcome as not-base announce, then better to use base
                Ok(base.hash)
            } else {
                Ok(not_base.hash)
            }
        }
        (Some(not_base), None) => Ok(not_base.hash),
        (None, Some(base)) => Ok(base.hash),
        (None, None) => Err(anyhow!(
            "No announces with parent {best_parent} found for block {block_hash}"
        )),
    }
}

/// Returns announce hash, which is supposed to be best among provided announces.
fn best_announce(
    db: &impl DBAnnouncesExt,
    announces: impl IntoIterator<Item = HashOf<Announce>>,
    limit: u32,
) -> Result<HashOf<Announce>> {
    let mut announces = announces.into_iter();
    let Some(first) = announces.next() else {
        return Err(anyhow!("No announces provided"));
    };

    let start_announce_hash = db.globals().start_announce_hash;

    let announce_points = |mut announce_hash| -> Result<u32> {
        let mut points = 0;
        for _ in 0..limit {
            let announce = db
                .announce(announce_hash)
                .ok_or_else(|| anyhow!("Announce {announce_hash} not found in db"))?;

            // Base announce gives 0 points, not-base - 1 point,
            // in order to prefer not-base announces, when select best chain.
            points += if announce.is_base() { 0 } else { 1 };

            if announce_hash == start_announce_hash {
                break;
            }

            announce_hash = announce.parent;
        }

        Ok(points)
    };

    let mut best_announce_hash = first;
    let mut best_announce_points = announce_points(first)?;
    for announce_hash in announces {
        let points = announce_points(announce_hash)?;

        if points > best_announce_points {
            best_announce_points = points;
            best_announce_hash = announce_hash;
        }
    }

    let best_announce = db
        .announce(best_announce_hash)
        .ok_or_else(|| anyhow!("Best announce {best_announce_hash} not found in db"))?;

    if best_announce.is_base() {
        // we can return it without checking siblings
        return Ok(best_announce_hash);
    }

    let Some(base_announce) = db.find_block_announce(best_announce.block_hash, |announce| {
        announce.value.is_base() && announce.value.parent == best_announce.parent
    })?
    else {
        return Ok(best_announce_hash);
    };

    if announces_have_equal_outcomes(db, base_announce.hash, best_announce_hash) {
        // if base announce has the same outcome as best announce, then better to use base
        Ok(base_announce.hash)
    } else {
        Ok(best_announce_hash)
    }
}

pub fn announces_have_equal_outcomes(
    db: &impl DBAnnouncesExt,
    announce1_hash: HashOf<Announce>,
    announce2_hash: HashOf<Announce>,
) -> bool {
    let outcome1 = db.announce_outcome(announce1_hash);
    let outcome2 = db.announce_outcome(announce2_hash);
    outcome1.is_some() && outcome1 == outcome2
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum AnnounceRejectionReason {
    #[display("Announce {announce_hash} parent {parent_announce_hash} is unknown")]
    UnknownParent {
        announce_hash: HashOf<Announce>,
        parent_announce_hash: HashOf<Announce>,
    },
    #[display("Announce {_0} is already included")]
    AlreadyIncluded(HashOf<Announce>),
    #[display("Invalid transactions: {_0:?}")]
    TxValidity(TxValidity),
    #[display("Announce touches too many programs: {_0}")]
    TooManyTouchedPrograms(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum AnnounceStatus {
    #[display("Announce {_0} accepted")]
    Accepted(HashOf<Announce>),
    #[display("Announce {announce} rejected: {reason:?}")]
    Rejected {
        announce: Announce,
        reason: AnnounceRejectionReason,
    },
}

/// Tries to accept provided announce: check it and include into database.
/// To be accepted, announce must
/// 1) announce parent must be included by this node.
/// 2) be not included yet.
///
/// Guarantee:
/// - caller must guaranty that announce block is known prepared block
pub fn accept_announce(db: &impl DBAnnouncesExt, announce: Announce) -> Result<AnnounceStatus> {
    let announce_hash = announce.to_hash();
    let parent_announce_hash = announce.parent;
    if !db.is_announce_included(parent_announce_hash) {
        return Ok(AnnounceStatus::Rejected {
            announce,
            reason: AnnounceRejectionReason::UnknownParent {
                announce_hash,
                parent_announce_hash,
            },
        });
    }

    let block = db
        .block_header(announce.block_hash)
        .map(|header| SimpleBlockData {
            hash: announce.block_hash,
            header,
        })
        .ok_or_else(|| {
            tracing::error!("Caller must guaranty that announce block is known prepared block");
            anyhow!("Announce block header not found")
        })?;

    // Verify for parent announce, because of the current is not processed.
    let tx_checker = TxValidityChecker::new_for_announce(db, block, announce.parent)?;

    for tx in announce.injected_transactions.iter() {
        let validity_status = tx_checker.check_tx_validity(tx)?;

        match validity_status {
            TxValidity::Valid => {
                db.set_injected_transaction(tx.clone());
            }

            validity => {
                tracing::trace!(
                    announce = ?announce.to_hash(),
                    "announce contains invalid transition with status {validity_status:?}, rejecting announce."
                );

                return Ok(AnnounceStatus::Rejected {
                    announce,
                    reason: AnnounceRejectionReason::TxValidity(validity),
                });
            }
        }
    }

    let (announce_hash, newly_included) = db.include_announce(announce.clone())?;
    if !newly_included {
        return Ok(AnnounceStatus::Rejected {
            announce,
            reason: AnnounceRejectionReason::AlreadyIncluded(announce_hash),
        });
    }

    let mut touched_programs = crate::utils::block_touched_programs(db, announce.block_hash)?;

    // Producer cannot avoid touching programs which are touched by block,
    // so we take as limit the number of touched programs in block, but not less than protocol limit.
    let limit = touched_programs
        .len()
        .max(MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE as usize);

    for tx in announce.injected_transactions.iter() {
        touched_programs.insert(tx.data().destination);
    }

    if touched_programs.len() > limit {
        return Ok(AnnounceStatus::Rejected {
            announce,
            reason: AnnounceRejectionReason::TooManyTouchedPrograms(touched_programs.len() as u32),
        });
    }

    Ok(AnnounceStatus::Accepted(announce_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx_validation::MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES;
    use ethexe_common::{
        StateHashWithQueueSize,
        db::*,
        events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
        gear::StateTransition,
        injected::InjectedTransaction,
        mock::*,
    };
    use ethexe_db::Database;
    use ethexe_runtime_common::state::{ActiveProgram, Program, ProgramState};
    use gear_core::program::MemoryInfix;
    use gprimitives::{ActorId, MessageId};
    use gsigner::{PrivateKey, SignedMessage};
    use proptest::{
        prelude::{Just, Strategy},
        proptest,
        test_runner::Config as ProptestConfig,
    };

    fn make_chain(last: usize, fnp: usize, wta: usize) -> BlockChain {
        let mut chain = BlockChain::mock(last as u32);
        (fnp..=last).for_each(|i| {
            chain.blocks[i]
                .as_prepared_mut()
                .announces
                .take()
                .iter()
                .flatten()
                .for_each(|announce_hash| {
                    chain.announces.remove(announce_hash);
                });
        });

        // append not-base announce at block with_two_announces
        let announce = Announce::with_default_gas(
            chain.blocks[wta].hash,
            chain.block_top_announce(wta).announce.parent,
        );
        let announce_hash = announce.to_hash();
        chain.blocks[wta]
            .as_prepared_mut()
            .announces
            .as_mut()
            .unwrap()
            .insert(announce_hash);
        chain.announces.insert(
            announce_hash,
            AnnounceData {
                announce,
                computed: None,
            },
        );

        chain
    }

    fn block_hash_and_announces_amount(
        db: &Database,
        chain: &BlockChain,
        idx: usize,
    ) -> (H256, usize) {
        let block_hash = chain.blocks[idx].hash;
        let announces_amount = db
            .block_meta(block_hash)
            .announces
            .unwrap_or_else(|| panic!("announces not found for block {block_hash}"))
            .len();
        (block_hash, announces_amount)
    }

    #[derive(Debug, Clone)]
    struct PropBaseParams {
        /// first not propagated block index in chain
        fnp: usize,
        /// last block index in chain
        last: usize,
        /// commitment delay limit
        cdl: usize,
        /// with two announces block index
        wta: usize,
    }

    fn base_params() -> impl Strategy<Value = PropBaseParams> {
        (2usize..=100)
            .prop_flat_map(|last| (2..=last, Just(last), 1usize..=1000))
            .prop_flat_map(|(fnp, last, cdl)| {
                Just(PropBaseParams {
                    fnp,
                    last,
                    cdl,
                    // only wta == fnp - 1 is supported in current tests
                    wta: fnp - 1,
                })
            })
    }

    fn base_params_and_committed_at() -> impl Strategy<Value = (PropBaseParams, usize)> {
        // committed_at - block where the missing announce was committed (wta + 1..=min(wta + cdl, last))
        base_params().prop_flat_map(|p| {
            let committed_at = (p.wta + 1)..=p.last.min(p.wta + p.cdl);
            (Just(p), committed_at)
        })
    }

    fn base_params_and_created_committed_at()
    -> impl Strategy<Value = (PropBaseParams, usize, usize)> {
        // created_at - block where the missing announce is created (fnp.saturating_sub(cdl)..fnp)
        // committed_at - Block where the missing announce is committed (fnp..=min(created_at + cdl, last))
        base_params()
            .prop_flat_map(|p| {
                let created_at = p.fnp.saturating_sub(p.cdl)..p.fnp;
                (Just(p), created_at)
            })
            .prop_flat_map(|(p, created_at)| {
                let committed_at = p.fnp..=p.last.min(created_at + p.cdl);
                (Just(p), Just(created_at), committed_at)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn proptest_propagation(p in base_params()) {
            let PropBaseParams { fnp, last, cdl, wta } = p;

            let db = Database::memory();
            let chain = make_chain(last, fnp, wta).setup(&db);

            let blocks = db
                .collect_blocks_without_announces(chain.blocks[last].hash)
                .unwrap();
            propagate_announces(&db, blocks, cdl as u32, Default::default()).unwrap();

            for i in 0..=last {
                let (block_hash, announces_amount) =
                    block_hash_and_announces_amount(&db, &chain, i);

                if i < wta {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                } else if i >= wta && i < wta + cdl {
                    assert_eq!(announces_amount, 2, "Block {i} {block_hash}");
                } else {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                }
            }
        }

        #[test]
        fn proptest_propagation_with_committed_announce(p in base_params()) {
            let PropBaseParams { fnp, last, cdl, wta } = p;

            let db = Database::memory();
            let mut chain = make_chain(last, fnp, wta);

            (fnp..=last).for_each(|i| {
                chain.blocks[i].as_prepared_mut().last_committed_announce =
                    chain.block_top_announce_hash(wta);
            });

            let chain = chain.setup(&db);

            let blocks = db
                .collect_blocks_without_announces(chain.blocks[last].hash)
                .unwrap();
            propagate_announces(&db, blocks, cdl as u32, Default::default()).unwrap();

            for i in 0..=last {
                let (block_hash, announces_amount) =
                    block_hash_and_announces_amount(&db, &chain, i);

                if i == wta {
                    assert_eq!(announces_amount, 2, "Block {i} {block_hash}");
                } else {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                }
            }

            assert_eq!(
                db.announce(db.top_announce_hash(chain.blocks[fnp].hash))
                    .unwrap()
                    .parent,
                chain.block_top_announce_hash(wta)
            );
        }

        #[test]
        fn proptest_propagation_committed_delayed((p, committed_at) in base_params_and_committed_at()) {
            let PropBaseParams { fnp, last, cdl, wta } = p;

            let db = Database::memory();
            let mut chain = make_chain(last, fnp, wta);

            let committed_announce_hash = chain.block_top_announce(wta).announce.to_hash();

            for i in committed_at..=last {
                chain.blocks[i].as_prepared_mut().last_committed_announce = committed_announce_hash;
            }

            let chain = chain.setup(&db);

            let blocks = db
                .collect_blocks_without_announces(chain.blocks[last].hash)
                .unwrap();
            propagate_announces(&db, blocks, cdl as u32, Default::default()).unwrap();

            for i in 0..=last {
                let (block_hash, announces_amount) =
                    block_hash_and_announces_amount(&db, &chain, i);

                if i < wta {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                } else if i >= wta && i < committed_at {
                    assert_eq!(announces_amount, 2, "Block {i} {block_hash}");
                } else {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                }
            }
        }

        #[test]
        fn proptest_propagation_missing((p, created_at, committed_at) in base_params_and_created_committed_at()) {
            let PropBaseParams { fnp, last, cdl, wta } = p;

            let db = Database::memory();
            let mut chain = make_chain(last, fnp, wta);

            let missing_announce = Announce {
                block_hash: chain.blocks[created_at].hash,
                parent: chain.block_top_announce(created_at).announce.parent,
                gas_allowance: Some(43),
                injected_transactions: Default::default()
            };
            let missing_announce_hash = missing_announce.to_hash();

            (committed_at..=last).for_each(|i| {
                chain.blocks[i].as_prepared_mut().last_committed_announce = missing_announce_hash;
            });

            let chain = chain.setup(&db);

            let blocks = db
                .collect_blocks_without_announces(chain.blocks[last].hash)
                .unwrap();
            propagate_announces(
                &db,
                blocks,
                cdl as u32,
                [(missing_announce_hash, missing_announce)]
                    .into_iter()
                    .collect(),
            )
            .unwrap();

            for i in 0..=last {
                let (block_hash, announces_amount) =
                    block_hash_and_announces_amount(&db, &chain, i);

                if i < created_at {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                } else if i >= created_at && i < wta {
                    assert_eq!(announces_amount, 2, "Block {i} {block_hash}");
                } else if i >= wta && i < committed_at {
                    assert_eq!(announces_amount, 3, "Block {i} {block_hash}");
                } else {
                    assert_eq!(announces_amount, 1, "Block {i} {block_hash}");
                }
            }
        }
    }

    #[test]
    fn reject_announce_with_too_many_touched_programs() {
        gear_utils::init_default_logger();

        let db = Database::memory();

        let state = ProgramState {
            program: Program::Active(ActiveProgram {
                allocations_hash: HashOf::zero().into(),
                pages_hash: HashOf::zero().into(),
                memory_infix: MemoryInfix::new(0),
                initialized: true,
            }),
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
            ..ProgramState::zero()
        };
        let state_hash = db.write_program_state(state);

        let chain = BlockChain::mock(10)
            .tap_mut(|chain| {
                chain.blocks[10].as_synced_mut().events =
                    (0..MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE / 2 + 1)
                        .map(|i| BlockEvent::Mirror {
                            actor_id: ActorId::from(i as u64),
                            event: MirrorEvent::MessageQueueingRequested(
                                MessageQueueingRequestedEvent {
                                    id: MessageId::zero(),
                                    source: ActorId::zero(),
                                    payload: vec![],
                                    value: 0,
                                    call_reply: false,
                                },
                            ),
                        })
                        .collect();

                chain
                    .block_top_announce_mut(9)
                    .as_computed_mut()
                    .program_states = (0..MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE + 1)
                    .map(|i| {
                        (
                            ActorId::from(i as u64),
                            StateHashWithQueueSize {
                                hash: state_hash,
                                canonical_queue_size: 0,
                                injected_queue_size: 0,
                            },
                        )
                    })
                    .collect();

                chain.globals.latest_computed_announce_hash = chain.block_top_announce_hash(9);
            })
            .setup(&db);

        let announce = Announce {
            block_hash: chain.blocks[10].hash,
            parent: chain.block_top_announce_hash(9),
            gas_allowance: Some(43),
            injected_transactions: (MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE / 2 + 1
                ..MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE + 1)
                .map(|i| InjectedTransaction {
                    destination: ActorId::from(i as u64),
                    payload: Default::default(),
                    value: 0,
                    reference_block: chain.blocks[10].hash,
                    salt: H256::random().0.to_vec().try_into().unwrap(),
                })
                .map(|tx| SignedMessage::create(PrivateKey::random(), tx).unwrap())
                .collect(),
        };

        let status = accept_announce(&db, announce.clone()).unwrap();
        let AnnounceStatus::Rejected { reason, .. } = status else {
            panic!("Announce should be rejected");
        };
        assert_eq!(
            reason,
            AnnounceRejectionReason::TooManyTouchedPrograms(MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE + 1)
        );
    }

    #[test]
    fn best_announce_prefers_base_sibling_with_same_outcome() {
        let db = Database::memory();

        let mut chain = BlockChain::mock(5);

        // Block 3 already has a base announce. Add a not-base sibling with the same parent.
        let base_hash = chain.block_top_announce_hash(3);
        let base_announce = &chain.block_top_announce(3).announce;
        let parent = base_announce.parent;
        let block_hash = base_announce.block_hash;

        let not_base_announce = Announce::with_default_gas(block_hash, parent);
        let not_base_hash = not_base_announce.to_hash();

        chain.blocks[3]
            .as_prepared_mut()
            .announces
            .as_mut()
            .unwrap()
            .insert(not_base_hash);

        // Both announces computed with the same (empty) outcome
        chain.announces.insert(
            not_base_hash,
            AnnounceData {
                announce: not_base_announce,
                computed: Some(MockComputedAnnounceData::default()),
            },
        );

        let chain = chain.setup(&db);

        // Not-base has more points (1 vs 0), but base sibling has the same outcome,
        // so best_announce should prefer the base one.
        let result = best_announce(&db, [not_base_hash, base_hash], 3).unwrap();
        assert_eq!(
            result, base_hash,
            "Should prefer base announce when sibling outcomes are the same"
        );

        // Also verify via best_parent_announce: block 4 should pick base at block 3 as best parent
        let best_parent_hash = best_parent_announce(&db, chain.blocks[4].hash, 3).unwrap();
        assert_eq!(
            best_parent_hash, base_hash,
            "best_parent_announce should prefer base parent with same outcome"
        );
    }

    #[test]
    fn best_announce_keeps_not_base_when_outcomes_differ() {
        let db = Database::memory();

        let mut chain = BlockChain::mock(5);

        let base_hash = chain.block_top_announce_hash(3);
        let base_announce = &chain.block_top_announce(3).announce;
        let parent = base_announce.parent;
        let block_hash = base_announce.block_hash;

        let not_base_announce = Announce::with_default_gas(block_hash, parent);
        let not_base_hash = not_base_announce.to_hash();

        chain.blocks[3]
            .as_prepared_mut()
            .announces
            .as_mut()
            .unwrap()
            .insert(not_base_hash);

        // Not-base announce has a different outcome (non-empty)
        chain.announces.insert(
            not_base_hash,
            AnnounceData {
                announce: not_base_announce,
                computed: Some(MockComputedAnnounceData {
                    outcome: vec![StateTransition {
                        actor_id: ActorId::from(1u64),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
        );

        let _chain = chain.setup(&db);

        // Not-base has more points AND different outcome, so it wins.
        let result = best_announce(&db, [not_base_hash, base_hash], 3).unwrap();
        assert_eq!(
            result, not_base_hash,
            "Should keep not-base announce when outcomes differ"
        );
    }

    #[test]
    fn best_announce_not_computed_keeps_not_base() {
        let db = Database::memory();

        let mut chain = BlockChain::mock(5);

        let base_hash = chain.block_top_announce_hash(3);
        let base_announce = &chain.block_top_announce(3).announce;
        let parent = base_announce.parent;
        let block_hash = base_announce.block_hash;

        let not_base_announce = Announce::with_default_gas(block_hash, parent);
        let not_base_hash = not_base_announce.to_hash();

        chain.blocks[3]
            .as_prepared_mut()
            .announces
            .as_mut()
            .unwrap()
            .insert(not_base_hash);

        // Not-base announce is NOT computed (computed: None)
        chain.announces.insert(
            not_base_hash,
            AnnounceData {
                announce: not_base_announce,
                computed: None,
            },
        );

        let _chain = chain.setup(&db);

        // Not-base has more points; sibling check returns NotComputed, so not-base wins.
        let result = best_announce(&db, [not_base_hash, base_hash], 3).unwrap();
        assert_eq!(
            result, not_base_hash,
            "Should keep not-base announce when sibling is not computed"
        );
    }
}
