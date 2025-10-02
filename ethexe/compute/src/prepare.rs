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

//!
//! All statements below correct only if majority of validators are correct ( > 2/3 )
//!
//! ## Statements
//!
//! ### STATEMENT1 (S1)
//! Any not base `announce` created by producer for `block` can be committed in `block1` only if
//! 1) `block1` is a strict successor of `block`
//! 2) `block1.height - block.height <= commitment_delay_limit`
//!
//! ### STATEMENT2 (S2)
//! If it's known at `block` that `announce1` has been committed
//! and `announce2` has committed after `announce1` then
//! 1) `announce2` is strict successor of `announce1`
//! 2) `announce2.block` is a strict successor of `announce1.block`,
//!    where `announce.block` is a block for which announce has been created.
//! 3) `announce2.committed_block` is a (not strict) successor of `announce1.committed_block`,
//!    where `announce.committed_block` is a block where announce has been committed.
//!
//! ## Theorems
//! > Belows are correct only if S1 and S2 are correct for the network
//!
//! ### Common definitions
//! - `block` - new received block from main-chain
//! - `lpb` - last prepared block, which is predecessor of `block`
//! - `chain` - ordered set of not prepared blocks till `block`
//! - `start_block` - network genesis or defined by fast_sync block,
//!   local main chain always starts from this block. Always has only one announce.
//!
//! ### THEOREM 1 (T1)
//! If `announce` is any announce committed in any blocks from `chain`
//! and `announce` is not yet computed by this node, then
//! exists computed `cpa` (common predecessor announce), which is
//! 1) predecessor of at least one announce from `lpb.announces`
//! 2) strict predecessor of `announce`
//! 3) `cpa.block.height > lpb.height - commitment_delay_limit`
//!
//! ### T1 Consequences
//! If `announce` is committed in some block from `chain` and
//! this `announce` is not computed by this node, then
//! 1) (T1S1) `announce.block.height > lpb.height - commitment_delay_limit`
//! 2) (T1S2) not computed announces chain len smaller than `chain.len() + commitment_delay_limit`
//! 3) (T1S3) If `announce1` is predecessor of any announce from `lpb.announces`
//!    and `announce1.block.height <= lpb.height - commitment_delay_limit`,
//!    then `announce1` is strict predecessor of `announce` and is predecessor of each
//!    announce from `lpb.announces`.

use crate::{ComputeError, ConsensusGuaranteesError, DataRequest, Result, utils};
use anyhow::anyhow;
use ethexe_common::{
    Announce, AnnounceHash, AnnouncesRequest, AnnouncesRequestUntil, CheckedAnnouncesResponse,
    SimpleBlockData,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMeta, BlockMetaStorageRead,
        BlockMetaStorageWrite, CodesStorageRead, LatestDataStorageRead, LatestDataStorageWrite,
        OnChainStorageRead,
    },
    events::{BlockEvent, RouterEvent},
};
use ethexe_db::Database;
use gprimitives::{CodeId, H256};
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    mem,
};

#[derive(Debug)]
pub(crate) struct PrepareContext {
    inner: PrepareContextInner,
    not_prepared_blocks_chain: VecDeque<SimpleBlockData>,
    required_data: RequiredData,
}

#[derive(Debug)]
struct PrepareContextInner {
    db: Database,
    commitment_delay_limit: u32,
    head: H256,
    collected_announces: HashMap<AnnounceHash, Announce>,
}

#[derive(Default, Debug)]
pub struct RequiredData {
    pub codes: HashSet<CodeId>,
    pub announces: Option<AnnouncesRequest>,
}

pub enum PrepareStatus {
    Prepared(H256),
    NotReady,
}

impl PrepareContext {
    pub fn new(
        db: Database,
        commitment_delay_limit: u32,
        head: H256,
    ) -> Result<(Self, DataRequest)> {
        if !db.block_synced(head) {
            return Err(ComputeError::BlockNotSynced(head));
        }

        let chain = utils::collect_chain(&db, head, |meta| !meta.prepared)?;

        let mut missing_codes = HashSet::new();
        let mut missing_validated_codes = HashSet::new();
        let mut last_committed_unknown_announce_hash = None;

        for block in chain.iter() {
            let events = db
                .block_events(block.hash)
                .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;

            for event in events {
                match event {
                    BlockEvent::Router(RouterEvent::CodeValidationRequested {
                        code_id, ..
                    }) if db.code_valid(code_id).is_none() => {
                        missing_codes.insert(code_id);
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid })
                        if db.code_valid(code_id).is_none() =>
                    {
                        if valid {
                            missing_validated_codes.insert(code_id);
                            missing_codes.insert(code_id);
                        } else {
                            // In case we receive code validation request first and then
                            // code got validation status false, then no need to load this code and
                            // process it, because it will never be used any more.
                            missing_codes.remove(&code_id);
                        }
                    }
                    BlockEvent::Router(RouterEvent::AnnouncesCommitted(head_announce_hash)) => {
                        // TODO +_+_+: optimize unknown base announces requests
                        // Even if only base announces was committed in blocks from `chain`,
                        // we still would request this announces, regardless of their base status.
                        if !utils::announce_is_included(&db, head_announce_hash) {
                            last_committed_unknown_announce_hash = Some(head_announce_hash);
                        }
                    }
                    _ => {}
                }
            }
        }

        let announces = if let Some(announce_hash) = last_committed_unknown_announce_hash {
            let last_prepared_block = chain
                .front()
                .expect(
                    "At least one block must be in chain if unknown committed announces was found",
                )
                .header
                .parent_hash;
            let tail = calculate_announces_common_predecessor(
                &db,
                commitment_delay_limit,
                last_prepared_block,
            )?;

            let announces_request = AnnouncesRequest {
                head: announce_hash,
                until: AnnouncesRequestUntil::Tail(tail),
            };

            Some(announces_request)
        } else {
            None
        };

        debug_assert!(
            missing_validated_codes
                .iter()
                .all(|code_id| missing_codes.contains(code_id)),
            "All missing validated codes must be in the missing codes list"
        );

        Ok((
            Self {
                inner: PrepareContextInner {
                    db,
                    commitment_delay_limit,
                    head,
                    collected_announces: Default::default(),
                },
                not_prepared_blocks_chain: chain,
                required_data: RequiredData {
                    codes: missing_validated_codes,
                    announces,
                },
            },
            DataRequest {
                codes: missing_codes,
                announces,
            },
        ))
    }

    pub fn receive_processed_code(&mut self, code_id: CodeId) {
        self.required_data.codes.remove(&code_id);
    }

    pub fn receive_announces(&mut self, announces: CheckedAnnouncesResponse) {
        let (request, response) = announces.into_parts();

        if Some(request) != self.required_data.announces {
            log::warn!("Receive announces response for unexpected request {request:?}");
            return;
        }

        for announce in response {
            self.inner
                .collected_announces
                .insert(announce.to_hash(), announce);
        }

        self.required_data.announces = None;
    }

    pub fn prepare_if_ready(&mut self) -> Result<PrepareStatus> {
        if !self.required_data.is_empty() {
            return Ok(PrepareStatus::NotReady);
        }

        let not_prepared_blocks_chain = mem::take(&mut self.not_prepared_blocks_chain);

        not_prepared_blocks_chain
            .into_iter()
            .try_for_each(|block| self.inner.prepare_one_block(block))
            .map(|_| PrepareStatus::Prepared(self.inner.head))
    }
}

impl PrepareContextInner {
    fn prepare_one_block(&mut self, block: SimpleBlockData) -> Result<()> {
        log::trace!("Preparing next block in chain: {}", block.hash);

        let parent = block.header.parent_hash;
        let mut requested_codes = HashSet::new();
        let mut validated_codes = HashSet::new();
        let mut last_committed_batch = None;
        let mut last_committed_announce_hash = None;

        let events = self
            .db
            .block_events(block.hash)
            .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;
        for event in events {
            match event {
                BlockEvent::Router(RouterEvent::BatchCommitted { digest }) => {
                    last_committed_batch = Some(digest);
                }
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                    requested_codes.insert(code_id);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    validated_codes.insert(code_id);
                }
                BlockEvent::Router(RouterEvent::AnnouncesCommitted(head)) => {
                    last_committed_announce_hash = Some(head);
                }
                _ => {}
            }
        }

        if let Some(hash) = last_committed_announce_hash {
            self.announces_chain_recovery_if_needed(hash)?;
        }

        let parent_meta = self.db.block_meta(parent);
        let last_committed_announce_hash = match last_committed_announce_hash {
            Some(hash) => hash,
            None => parent_meta
                .last_committed_announce
                .ok_or(ComputeError::LastCommittedHeadNotFound(parent))?,
        };
        let last_committed_batch = match last_committed_batch {
            Some(digest) => digest,
            None => parent_meta
                .last_committed_batch
                .ok_or(ComputeError::LastCommittedBatchNotFound(parent))?,
        };

        // +_+_+ #4813 #4814 are fixed now
        let mut new_announces = BTreeSet::new();
        for parent_announce_hash in parent_meta
            .announces
            .ok_or(ComputeError::PreparedBlockAnnouncesSetMissing(parent))?
        {
            if let Some(new_announce_hash) = self.propagate_from_parent_announce(
                block.hash,
                parent_announce_hash,
                last_committed_announce_hash,
            )? && !new_announces.insert(new_announce_hash)
            {
                // Each announce should be unique, because parent announces are from BTreeSet and unique
                unreachable!("Duplicate base announce detected: {new_announce_hash}");
            };
        }

        let last_announce = new_announces.last().cloned().ok_or_else(|| {
            log::error!("No announces could be propagated for block {}", block.hash);
            // This error could occur only if old not base announce was committed
            ConsensusGuaranteesError::CommitmentDelayLimitExceeded
        })?;

        let mut codes_queue = parent_meta
            .codes_queue
            .ok_or(ComputeError::CodesQueueNotFound(parent))?;
        codes_queue.extend(requested_codes);
        codes_queue.retain(|code_id| !validated_codes.contains(code_id));

        self.db.mutate_block_meta(block.hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some(new_announces),
                codes_queue: Some(codes_queue),
                last_committed_batch: Some(last_committed_batch),
                last_committed_announce: Some(last_committed_announce_hash),
            };
        });

        self.db
            .mutate_latest_data(|data| {
                data.prepared_block_hash = block.hash;
                data.computed_announce_hash = last_announce;
            })
            .ok_or(ComputeError::LatestDataNotFound)?;

        Ok(())
    }

    /// Create a new base announce from provided parent announce hash.
    /// Compute the announce and store related data in the database.
    fn propagate_from_parent_announce(
        &mut self,
        block_hash: H256,
        parent_announce_hash: AnnounceHash,
        last_committed_announce_hash: AnnounceHash,
    ) -> Result<Option<AnnounceHash>> {
        log::trace!(
            "Trying propagating announce for block {block_hash} from parent announce {parent_announce_hash}, \
             last committed announce is {last_committed_announce_hash}",
        );

        // Check that parent announce branch is not expired
        // The branch is expired if:
        // 1. It does not includes last committed announce
        // 2. If it includes not committed and not base announce, which is older than commitment delay limit.
        //
        // We check here till commitment delay limit, because T1 guaranties that enough.
        let mut predecessor = parent_announce_hash;
        for i in 0..=self.commitment_delay_limit {
            if predecessor == last_committed_announce_hash {
                // We found last committed announce in the branch, until commitment delay limit
                // that means this branch is still not expired.
                break;
            }

            let predecessor_announce = self
                .db
                .announce(predecessor)
                .ok_or_else(|| ComputeError::AnnounceNotFound(predecessor))?;

            if i == self.commitment_delay_limit - 1 && !predecessor_announce.is_base() {
                // We reached the oldest announce in commitment delay limit which is not not committed yet.
                // This announce cannot be committed any more if it is not base announce,
                // so this branch is expired and we have to skip propagation from `parent`.
                log::trace!(
                    "predecessor {predecessor} is too old and not base, so {parent_announce_hash} branch is expired",
                );
                return Ok(None);
            }

            // Check neighbor announces to be last committed announce
            if self
                .db
                .block_meta(predecessor_announce.block_hash)
                .announces
                .ok_or_else(|| {
                    ComputeError::PreparedBlockAnnouncesSetMissing(predecessor_announce.block_hash)
                })?
                .contains(&last_committed_announce_hash)
            {
                // We found last committed announce in the neighbor branch, until commitment delay limit
                // that means this branch is already expired.
                return Ok(None);
            };

            predecessor = predecessor_announce.parent;
        }

        log::trace!("branch from {parent_announce_hash} is not expired, propagating announce");

        // +_+_+ debug_assert
        if !ethexe_common::announce_is_successor_of(
            &self.db,
            parent_announce_hash,
            last_committed_announce_hash,
        )
        .map_err(|err| {
            ComputeError::Other(anyhow!(
                "Failed to verify that announce {parent_announce_hash}
                    is successor of last committed announce {last_committed_announce_hash}: {err}"
            ))
        })? {
            let chain1: String =
                ethexe_common::announces_chain(&self.db, parent_announce_hash, None)?
                    .into_iter()
                    .map(|a| format!("({} {}) ", a.to_hash(), a.is_base()))
                    .collect();
            let chain2: String =
                ethexe_common::announces_chain(&self.db, last_committed_announce_hash, None)?
                    .into_iter()
                    .map(|a| format!("({} {}) ", a.to_hash(), a.is_base()))
                    .collect();

            log::error!("Chain1: {chain1}");
            log::error!("Chain2: {chain2}");

            return Err(ComputeError::Other(anyhow!(
                "Announce {parent_announce_hash} is not successor of last committed announce {last_committed_announce_hash}"
            )));
        }

        let new_base_announce = Announce::base(block_hash, parent_announce_hash);
        let new_base_announce_hash = self.db.set_announce(new_base_announce);

        Ok(Some(new_base_announce_hash))
    }

    fn announces_chain_recovery_if_needed(
        &mut self,
        last_committed_announce_hash: AnnounceHash,
    ) -> Result<()> {
        // Include chain of announces, which are not included yet
        let mut announce_hash = last_committed_announce_hash;
        while !utils::announce_is_included(&self.db, announce_hash) {
            log::debug!("Committed announces was not included yet, including...");

            let announce = self
                .collected_announces
                .remove(&announce_hash)
                .ok_or_else(|| {
                    anyhow!("Committed announce {announce_hash} not found in collected announces")
                })?;

            announce_hash = announce.parent;

            utils::include_one(&self.db, announce)?;
        }

        Ok(())
    }
}

impl RequiredData {
    pub fn is_empty(&self) -> bool {
        let RequiredData { codes, announces } = self;
        codes.is_empty() && announces.is_none()
    }
}

/// Returns announce hash from T1S3 or global genesis/start announce
fn calculate_announces_common_predecessor(
    db: &Database,
    commitment_delay_limit: u32,
    block_hash: H256,
) -> Result<AnnounceHash> {
    let start_announce = db
        .latest_data()
        .ok_or(ComputeError::LatestDataNotFound)?
        .start_announce_hash;

    let mut announces = db
        .block_meta(block_hash)
        .announces
        .ok_or(ComputeError::PreparedBlockAnnouncesSetMissing(block_hash))?
        .into_iter()
        .collect::<HashSet<_>>();

    log::trace!(
        "+_+_+ starting common predecessor search for block {block_hash} from announces: {announces:?}",
    );

    for _ in 0..commitment_delay_limit {
        if announces.len() == 1 && announces.contains(&start_announce) {
            return Ok(start_announce);
        }

        announces = announces
            .into_iter()
            .map(|announce_hash| {
                log::trace!("+_+_+ checking predecessor {announce_hash} for common predecessor");
                db.announce(announce_hash)
                    .map(|a| a.parent)
                    .ok_or(ComputeError::AnnounceNotFound(announce_hash))
            })
            .collect::<Result<HashSet<_>>>()?;
    }

    if let Some(&announce) = announces.iter().next()
        && announces.len() == 1
    {
        return Ok(announce);
    }

    // If we reached this point - common predecessor not found by some reasons
    // This can happen for example, if some old not base announce was committed
    // and T1S3 cannot be applied.
    Err(ConsensusGuaranteesError::Other(format!(
        "By some reasons common announces predecessor was not found for block {block_hash:?}"
    ))
    .into())
}

#[cfg(test)]
mod tests {
    use crate::utils::announce_is_included;

    use super::*;
    use ethexe_common::{Address, BlockHeader, Digest, db::*, events::BlockEvent};
    use ethexe_db::Database as DB;
    use gprimitives::H256;
    use nonempty::nonempty;

    // +_+_+ fix
    // #[tokio::test]
    // async fn test_propagate_data_from_parent() {
    //     let db = DB::memory();
    //     let block_hash = H256::random();
    //     let parent_announce_hash = AnnounceHash::random();

    //     db.set_block_events(block_hash, &[]);

    //     let announce_hash = propagate_from_parent_announce(
    //         &db,
    //         &mut MockProcessor,
    //         block_hash,
    //         parent_announce_hash,
    //         None,
    //     )
    //     .await
    //     .unwrap();
    //     assert_eq!(
    //         db.announce(announce_hash).unwrap(),
    //         Announce::new_default_gas(block_hash, parent_announce_hash),
    //         "incorrect announce was stored"
    //     );
    //     assert_eq!(db.announce_outcome(announce_hash), Some(Default::default()));
    //     assert_eq!(
    //         db.announce_schedule(announce_hash),
    //         Some(Default::default())
    //     );
    //     assert_eq!(
    //         db.announce_program_states(announce_hash),
    //         Some(Default::default())
    //     );
    //     assert!(db.announce_meta(announce_hash).computed);
    // }

    #[tokio::test]
    async fn test_prepare_one_block() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let parent_hash = H256::random();
        let block = SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 1,
                timestamp: 1000,
                parent_hash,
            },
        };
        let last_committed_announce = AnnounceHash::random();
        let code1_id = CodeId::from([1u8; 32]);
        let code2_id = CodeId::from([2u8; 32]);
        let batch_committed = Digest::random();
        let validators = nonempty![Address::from([42u8; 20])];

        let parent_announce = Announce::base(parent_hash, last_committed_announce);
        db.set_announce(parent_announce.clone());
        db.set_announce_outcome(parent_announce.to_hash(), Default::default());
        db.set_announce_schedule(parent_announce.to_hash(), Default::default());
        db.set_announce_program_states(parent_announce.to_hash(), Default::default());

        db.mutate_block_meta(parent_hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some([parent_announce.to_hash()].into()),
                codes_queue: Some([code1_id].into()),
                last_committed_batch: Some(Digest::random()),
                last_committed_announce: Some(AnnounceHash::random()),
            }
        });
        db.set_block_validators(parent_hash, validators.clone());

        db.set_block_header(block.hash, block.header);

        db.set_latest_data(Default::default());

        let events = vec![
            BlockEvent::Router(RouterEvent::BatchCommitted {
                digest: batch_committed,
            }),
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(parent_announce.to_hash())),
            BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id: code1_id,
                valid: true,
            }),
            BlockEvent::Router(RouterEvent::CodeValidationRequested {
                code_id: code2_id,
                timestamp: 1000,
                tx_hash: H256::random(),
            }),
        ];
        db.set_block_events(block.hash, &events);
        db.set_block_validators(block.hash, validators);
        db.set_block_synced(block.hash);

        PrepareContextInner {
            db: db.clone(),
            commitment_delay_limit: 3,
            head: block.hash,
            collected_announces: Default::default(),
        }
        .prepare_one_block(block.clone())
        .unwrap();

        let meta = db.block_meta(block.hash);
        assert!(meta.prepared);
        assert_eq!(meta.codes_queue, Some(vec![code2_id].into()),);
        assert_eq!(meta.last_committed_batch, Some(batch_committed),);
        assert_eq!(
            meta.last_committed_announce,
            Some(parent_announce.to_hash())
        );
        assert_eq!(meta.announces.as_ref().map(|a| a.len()), Some(1));

        let announce_hash = meta.announces.unwrap().first().copied().unwrap();
        let announce = db.announce(announce_hash).unwrap();
        assert_eq!(
            announce,
            Announce::base(block.hash, parent_announce.to_hash())
        );
        assert!(!db.announce_meta(announce_hash).computed);
        assert_eq!(db.announce_outcome(announce_hash), None);
        assert_eq!(db.announce_schedule(announce_hash), None);
        assert_eq!(db.announce_program_states(announce_hash), None);
        assert!(announce_is_included(&db, announce_hash));
        assert_eq!(db.latest_data().unwrap().prepared_block_hash, block.hash);
    }
}
