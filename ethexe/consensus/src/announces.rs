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
//! - `announce.block` - block for which announce was created.
//! - `announce.committed_block` - block where announce was committed (if it was committed).
//! - `announce.branch` - linked chain of announces starting from `start_announce` to `announce` itself.
//! - `base announce` - announce which does not have any injected transactions and gas allowance.
//! - `not-base announce` - any announce which is cannot be classified as base announce.
//! - `commitment_delay_limit` - protocol parameter defining maximal delay (in blocks)
//!   for committing announces not-base announces.
//! - `start_block` - genesis block (for ethexe) or defined by fast_sync block,
//!   It's guaranteed that it's predecessor of any new chain head coming from ethereum.
//!   Always has only one announce, which is called `start_announce`.
//! - `block.announces` - set of announces connected to the `block`. All announces in this set
//!   are created for this `block`.
//! - `included announce` - announce which has been included in `block.announces` of `announce.block`.
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
//! 2) `announce2.block` is a strict successor of `announce1.block`
//! 3) `announce2.committed_block` is a successor of `announce1.committed_block`
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
//! and `announce` is not yet included by this node, then `cpa` must exists
//! (common predecessor announce), which is
//! 1) included by this node
//! 2) strict predecessor of `announce`
//! 3) strict predecessor of at least one announce from `lpb.announces`
//! 4) `lpb.height - commitment_delay_limit < cpa.block.height < lpb.height`
//!
//! ### T1 Consequences
//! If `announce` is committed in some block from `chain` and
//! this `announce` is not included yet, then
//! 1) (T1S1) `announce.block.height > lpb.height - commitment_delay_limit`
//! 2) (T1S2) if `announce1` is predecessor of any announce from `lpb.announces`
//!    and `announce1.block.height <= lpb.height - commitment_delay_limit`,
//!    then `announce1` is strict predecessor of `announce` and is predecessor of each
//!    announce from `lpb.announces`.

use anyhow::{Result, anyhow, ensure};
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRW, BlockMetaStorageRW, LatestDataStorageRO, OnChainStorageRO},
    network::{AnnouncesRequest, AnnouncesRequestUntil},
};
use ethexe_ethereum::primitives::map::HashMap;
use gprimitives::H256;
use std::collections::{BTreeSet, VecDeque};

pub trait DBAnnouncesExt:
    AnnounceStorageRW + BlockMetaStorageRW + OnChainStorageRO + LatestDataStorageRO
{
    /// Collects blocks from the chain head backwards till the first propagated block found.
    fn collect_blocks_without_announces(&self, head: H256) -> Result<VecDeque<SimpleBlockData>>;

    /// Include announce into the database and link it to its block.
    /// Returns (announce_hash, is_newly_included).
    /// - `announce_hash` is the hash of the included announce.
    /// - `is_newly_included` is true if the announce was not included before, false otherwise.
    fn include_announce(&self, announce: Announce) -> Result<(HashOf<Announce>, bool)>;

    /// Check whether announce is already included.
    fn announce_is_included(&self, announce_hash: HashOf<Announce>) -> bool;

    /// Get set of parents for the given set of announces.
    fn announces_parents(
        &self,
        announces: impl IntoIterator<Item = HashOf<Announce>>,
    ) -> Result<BTreeSet<HashOf<Announce>>>;
}

impl<DB: AnnounceStorageRW + BlockMetaStorageRW + OnChainStorageRO + LatestDataStorageRO>
    DBAnnouncesExt for DB
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

    fn announce_is_included(&self, announce_hash: HashOf<Announce>) -> bool {
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

        announces_chain_recovery_if_needed(
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
fn announces_chain_recovery_if_needed(
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
    while count < commitment_delay_limit && !db.announce_is_included(current_announce_hash) {
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
        db.announce_is_included(current_announce_hash),
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
    let mut predecessor = parent_announce_hash;
    for i in 0..=commitment_delay_limit {
        if predecessor == last_committed_announce_hash {
            // We found last committed announce in the branch, until commitment delay limit
            // that means this branch is still not expired.
            break;
        }

        let predecessor_announce = db
            .announce(predecessor)
            .ok_or_else(|| anyhow!("announce({predecessor}) not found"))?;

        if i == commitment_delay_limit - 1 && !predecessor_announce.is_base() {
            // We reached the oldest announce in commitment delay limit which is not not committed yet.
            // This announce cannot be committed any more if it is not-base announce,
            // so this branch is expired and we have to skip propagation from `parent`.
            tracing::trace!(
                predecessor = %predecessor,
                parent_announce = %parent_announce_hash,
                "predecessor is too old and not-base, so parent announce branch is expired",
            );
            return Ok(None);
        }

        // Check neighbor announces to be last committed announce
        if db
            .block_meta(predecessor_announce.block_hash)
            .announces
            .ok_or_else(|| {
                anyhow!(
                    "announces are missing for block({})",
                    predecessor_announce.block_hash
                )
            })?
            .contains(&last_committed_announce_hash)
        {
            // We found last committed announce in the neighbor branch, until commitment delay limit
            // that means this branch is already expired.
            tracing::trace!(
                predecessor = %predecessor,
                parent_announce = %parent_announce_hash,
                latest_committed_announce = %last_committed_announce_hash,
                "neighbor announce branch contains last committed announce, so parent announce branch is expired",
            );
            return Ok(None);
        };

        predecessor = predecessor_announce.parent;
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

    if db.announce_is_included(last_committed_announce_hash) {
        // announce is already included, no need to request announces

        #[cfg(debug_assertions)]
        {
            // debug check that all announces in the chain are present (check only up to 100 announces)
            let start_announce_hash = db
                .latest_data()
                .expect("Latest data not found")
                .start_announce_hash;

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
                    db.announce_is_included(announce_hash),
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
    let start_announce_hash = db
        .latest_data()
        .ok_or_else(|| anyhow!("Latest data not found"))?
        .start_announce_hash;

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
    // We do not take announces directly from parent block,
    // because some of them may be expired at `block_hash`,
    // so we take parents of all announces from `block_hash`,
    // to be sure that we take only not expired parent announces.
    let parent_announces =
        db.announces_parents(db.block_meta(block_hash).announces.into_iter().flatten())?;

    best_announce(db, parent_announces, commitment_delay_limit)
}

/// Returns announce hash, which is supposed to be best among provided announces.
pub fn best_announce(
    db: &impl DBAnnouncesExt,
    announces: impl IntoIterator<Item = HashOf<Announce>>,
    commitment_delay_limit: u32,
) -> Result<HashOf<Announce>> {
    let mut announces = announces.into_iter();
    let Some(first) = announces.next() else {
        return Err(anyhow!("No announces provided"));
    };

    let start_announce_hash = db
        .latest_data()
        .ok_or_else(|| anyhow!("Latest data not found"))?
        .start_announce_hash;

    let announce_points = |mut announce_hash| -> Result<u32> {
        let mut points = 0;
        for _ in 0..commitment_delay_limit {
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

    Ok(best_announce_hash)
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum AnnounceRejectionReason {
    #[display("Unsuitable parent: expected {expected:?}, found {found:?}")]
    UnsuitableParent {
        expected: HashOf<Announce>,
        found: HashOf<Announce>,
    },
    #[display("Announce {_0} is already included")]
    AlreadyIncluded(HashOf<Announce>),
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum AnnounceStatus {
    #[display("Announce {_0} accepted")]
    Accepted(HashOf<Announce>),
    #[display("Announce {announce:?} rejected: {reason:?}")]
    Rejected {
        announce: Announce,
        reason: AnnounceRejectionReason,
    },
}

/// Tries to accept provided announce: check it and include into database.
/// To be accepted, announce must
/// 1) have suitable parent announce - currently it must be best (see [`best_parent_announce`]).
/// 2) be not included yet.
pub fn accept_announce(
    db: &impl DBAnnouncesExt,
    announce: Announce,
    commitment_delay_limit: u32,
) -> Result<AnnounceStatus> {
    let best_parent = best_parent_announce(db, announce.block_hash, commitment_delay_limit)?;
    let announce_parent = announce.parent;
    if best_parent != announce_parent {
        return Ok(AnnounceStatus::Rejected {
            announce,
            reason: AnnounceRejectionReason::UnsuitableParent {
                expected: best_parent,
                found: announce_parent,
            },
        });
    }

    let (announce_hash, newly_included) = db.include_announce(announce.clone())?;
    if newly_included {
        Ok(AnnounceStatus::Accepted(announce_hash))
    } else {
        Ok(AnnounceStatus::Rejected {
            announce,
            reason: AnnounceRejectionReason::AlreadyIncluded(announce_hash),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{db::*, mock::*};
    use ethexe_db::Database;
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
                off_chain_transactions: Default::default()
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
}
