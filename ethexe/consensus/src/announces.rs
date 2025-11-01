use anyhow::{Result, anyhow, ensure};
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRW, BlockMetaStorageRW, LatestDataStorageRO, OnChainStorageRO},
    network::{AnnouncesRequest, AnnouncesRequestUntil},
};
use ethexe_ethereum::primitives::map::HashMap;
use gprimitives::H256;
use std::collections::{BTreeSet, VecDeque};

pub trait DBExt:
    AnnounceStorageRW + BlockMetaStorageRW + OnChainStorageRO + LatestDataStorageRO
{
    fn collect_blocks_without_announces(&self, head: H256) -> Result<VecDeque<SimpleBlockData>>;
    fn include_announce(&self, announce: Announce) -> Result<HashOf<Announce>>;
    fn announce_is_included(&self, announce_hash: HashOf<Announce>) -> bool;
    fn announces_parents(
        &self,
        announces: impl IntoIterator<Item = HashOf<Announce>>,
    ) -> Result<BTreeSet<HashOf<Announce>>>;
}

impl<DB: AnnounceStorageRW + BlockMetaStorageRW + OnChainStorageRO + LatestDataStorageRO> DBExt
    for DB
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

    fn include_announce(&self, announce: Announce) -> Result<HashOf<Announce>> {
        tracing::trace!(announce = %announce.to_hash(), "Including announce...");

        let block_hash = announce.block_hash;
        let announce_hash = self.set_announce(announce);

        let mut not_yet_included = true;
        self.mutate_block_meta(block_hash, |meta| {
            not_yet_included = meta.announces.get_or_insert_default().insert(announce_hash);
        });

        not_yet_included.then_some(announce_hash).ok_or_else(|| {
            anyhow!("announce {announce_hash} for block {block_hash} was already included")
        })
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

pub fn propagate_announces(
    db: &impl DBExt,
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
            propagate_one_base_announce(
                db,
                block.hash,
                parent_announce_hash,
                last_committed_announce_hash,
                commitment_delay_limit,
            )?;
        }

        debug_assert!(
            db.block_meta(block.hash)
                .announces
                .into_iter()
                .flatten()
                .next()
                .is_some(),
            "at least one announce must be propagated for block({})",
            block.hash
        );
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
/// where `(A3')` and `(A2')` are committed
/// and must be presented in `missing_announces` if they are not included yet,
/// and `(A4')` is base announce propagated from `(A3')`.
fn announces_chain_recovery_if_needed(
    db: &impl DBExt,
    block: &SimpleBlockData,
    last_committed_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
    missing_announces: &mut HashMap<HashOf<Announce>, Announce>,
) -> Result<()> {
    // TODO +_+_+: append recovery from rejected announces
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

        db.include_announce(announce)?;
    }

    let Some(last_committed_announce_block_hash) = last_committed_announce_block_hash else {
        // No committed announces were missing, no need to recover
        return Ok(());
    };

    // If error: DB is corrupted, or incorrect commitment detected (have not base announce committed after commitment delay limit)
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

    // If error: DB is corrupted, or incorrect commitment detected (have not base announce committed after commitment delay limit)
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
        parent_announce_hash = db.include_announce(new_base_announce)?;
    }

    Ok(())
}

/// Create a new base announce from provided parent announce hash.
/// Compute the announce and store related data in the database.
fn propagate_one_base_announce(
    db: &impl DBExt,
    block_hash: H256,
    parent_announce_hash: HashOf<Announce>,
    last_committed_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
) -> Result<()> {
    tracing::trace!(
        block = %block_hash,
        parent_announce = %parent_announce_hash,
        last_committed_announce = %last_committed_announce_hash,
        "Trying propagating announce from parent announce",
    );

    // Check that parent announce branch is not expired
    // The branch is expired if:
    // 1. It does not includes last committed announce
    // 2. If it includes not committed and not base announce, which is older than commitment delay limit.
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
            // This announce cannot be committed any more if it is not base announce,
            // so this branch is expired and we have to skip propagation from `parent`.
            tracing::trace!(
                predecessor = %predecessor,
                parent_announce = %parent_announce_hash,
                "predecessor is too old and not base, so parent announce branch is expired",
            );
            return Ok(());
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
            return Ok(());
        };

        predecessor = predecessor_announce.parent;
    }

    let new_base_announce = Announce::base(block_hash, parent_announce_hash);

    tracing::trace!(
        parent_announce = %parent_announce_hash,
        new_base_announce = %new_base_announce.to_hash(),
        "branch from parent announce is not expired, propagating new base announce",
    );

    db.include_announce(new_base_announce)?;

    Ok(())
}

pub fn check_for_missing_announces(
    db: &impl DBExt,
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
        // announce is unknown, or not included, so there can be missing announces
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

/// Returns announce hash from T1S3 or global start announce
fn find_announces_common_predecessor(
    db: &impl DBExt,
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
        // This can happen for example, if some old not base announce was committed
        // and T1S3 cannot be applied.
        Err(anyhow!(
            "Common predecessor for announces in block {block_hash} in nearest {commitment_delay_limit} blocks not found",
        ))
    }
}

/// Returns announce hash, which is supposed to be best to produce a new announce above.
/// Used to produce new announce or validate announce from producer.
pub fn best_parent_announce(
    db: &impl DBExt,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<HashOf<Announce>> {
    // We do not take announces directly from parent announces,
    // because some of them may be expired at `block_hash`.
    let parent_announces =
        db.announces_parents(db.block_meta(block_hash).announces.into_iter().flatten())?;

    best_announce(db, parent_announces, commitment_delay_limit)
}

pub fn best_announce(
    db: &impl DBExt,
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

            // Base announce gives 0 points, non-base - 1 point.
            // To prefer non-base announces, when select best chain.
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

pub enum AnnounceStatus {
    Accepted(HashOf<Announce>),
    Rejected { announce: Announce, reason: String },
}

pub fn accept_announce(
    db: &impl DBExt,
    announce: Announce,
    commitment_delay_limit: u32,
) -> Result<AnnounceStatus> {
    let best_parent = best_parent_announce(db, announce.block_hash, commitment_delay_limit)?;
    if best_parent != announce.parent {
        return Ok(AnnounceStatus::Rejected {
            announce,
            reason: format!("best parent is {best_parent}"),
        });
    }

    match db.include_announce(announce.clone()) {
        Ok(announce_hash) => Ok(AnnounceStatus::Accepted(announce_hash)),
        Err(err) => Ok(AnnounceStatus::Rejected {
            announce,
            reason: format!("{err}"),
        }),
    }
}
