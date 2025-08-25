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
//!     where `announce.block` is a block for which announce has been created.
//! 3) `announce2.committed_block` is a (not strict) successor of `announce1.committed_block`,
//!     where `announce.committed_block` is a block where announce has been committed.
//!
//! ## Theorems
//! > Belows are correct only if S1 and S2 are correct for the network
//!
//! ### Common definitions
//! - `block` - new received block from main-chain
//! - `lpb` - last prepared block, which is predecessor of `block`
//! - `chain` - ordered set of not prepared blocks till `block`
//! - `start_block` - network genesis or defined by fast_sync block,
//! local main chain always starts from this block. Always has only one announce.
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
//! 1) `announce.block.height > lpb.height - commitment_delay_limit`
//! 2) not computed announces chain len smaller than `chain.len() + commitment_delay_limit`
//! 3) If `announce1` is predecessor of any announce from `lpb.announces`
//! and `announce1.block.height <= lpb.height - commitment_delay_limit`,
//! then `announce1` is strict predecessor of `announce` and is predecessor of each
//! announce from `lpb.announces`.

use crate::{ComputeError, ConsensusGuaranteesError, ProcessorExt, Result, compute, utils};
use ethexe_common::{
    Announce, AnnounceHash, AnnouncesRequest, CheckedAnnouncesResponse, SimpleBlockData,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMeta, BlockMetaStorageRead,
        BlockMetaStorageWrite, CodesStorageRead, LatestDataStorageRead, LatestDataStorageWrite,
        OnChainStorageRead,
    },
    events::{BlockEvent, BlockRequestEvent, RouterEvent},
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;
use gprimitives::{CodeId, H256};
use std::collections::{HashSet, VecDeque};

pub(crate) struct PrepareConfig<P: ProcessorExt> {
    pub db: Database,
    pub processor: P,
    pub commitment_delay_limit: u32,
}

pub(crate) struct PrepareContext<P: ProcessorExt> {
    cfg: PrepareConfig<P>,
    block_hash: H256,
    chain: VecDeque<SimpleBlockData>,
    required_data: RequiredData,
}

#[derive(Default, Debug)]
pub(crate) struct MissingData {
    pub codes: HashSet<CodeId>,
    pub required: RequiredData,
}

#[derive(Default, Debug)]
pub(crate) struct RequiredData {
    pub codes: HashSet<CodeId>,
    pub announces: Option<AnnouncesRequest>,
}

impl RequiredData {
    pub fn is_empty(&self) -> bool {
        let RequiredData { codes, announces } = self;
        codes.is_empty() && announces.is_none()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DataRequest {
    pub codes: HashSet<CodeId>,
    pub announces: Option<AnnouncesRequest>,
}

impl DataRequest {
    pub fn is_empty(&self) -> bool {
        let DataRequest { codes, announces } = self;
        codes.is_empty() && announces.is_none()
    }
}

impl<P: ProcessorExt> PrepareContext<P> {
    pub fn new(cfg: PrepareConfig<P>, block_hash: H256) -> Result<(Self, DataRequest)> {
        let (MissingData { codes, required }, chain) = collect_missing_data(&cfg, block_hash)?;
        let data_request = DataRequest {
            codes: codes,
            announces: required.announces.clone(),
        };

        Ok((
            Self {
                cfg,
                block_hash,
                chain,
                required_data: required,
            },
            data_request,
        ))
    }

    pub fn code_processed(&mut self, code_id: CodeId) {
        self.required_data.codes.remove(&code_id);
    }

    pub fn receive_announces(&mut self, announces: CheckedAnnouncesResponse) {
        let (request, response) = announces.into_parts();

        if Some(request) != self.required_data.announces {
            log::warn!("Receive announces response for unexpected request {request:?}");
            return;
        }

        for announce in response {
            self.cfg.db.set_announce(announce);
        }

        self.required_data.announces = None;
    }

    pub fn is_ready(&self) -> bool {
        self.required_data.is_empty()
    }

    pub async fn prepare(self) -> Result<H256> {
        let Self {
            mut cfg,
            block_hash,
            chain,
            required_data,
        } = self;

        if !required_data.is_empty() {
            unreachable!(
                "PrepareContext::prepare must be called only when all required data loaded"
            );
        };

        debug_assert!(
            cfg.db.block_synced(block_hash),
            "Block {block_hash} must be synced, checked in missing data",
        );
        debug_assert!(
            {
                let (MissingData { required, .. }, new_chain) =
                    collect_missing_data(&cfg, block_hash).expect("Cannot collect missing data");
                chain == new_chain && required.is_empty()
            },
            "All required data must be loaded before calling prepare and no blocks can be prepared since"
        );

        for block in chain {
            prepare_one_block(&mut cfg, block).await?;
        }

        Ok(block_hash)
    }
}

fn collect_missing_data(
    cfg: &PrepareConfig<impl ProcessorExt>,
    block_hash: H256,
) -> Result<(MissingData, VecDeque<SimpleBlockData>)> {
    let &PrepareConfig {
        ref db,
        commitment_delay_limit,
        ..
    } = cfg;

    if !db.block_synced(block_hash) {
        return Err(ComputeError::BlockNotSynced(block_hash));
    }

    let chain = utils::collect_chain(db, block_hash, |meta| !meta.prepared)?;

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();
    let mut last_committed_unknown_announce_hash = None;

    for block in chain.iter() {
        let events = db
            .block_events(block.hash)
            .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;

        for event in events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. })
                    if db.code_valid(code_id).is_none() =>
                {
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
                    if !db.announce_meta(head_announce_hash).computed {
                        last_committed_unknown_announce_hash = Some(head_announce_hash);
                    }
                }
                _ => {}
            }
        }
    }

    let announces = if let Some(announce_hash) = last_committed_unknown_announce_hash {
        // see T1 sequence 2
        let max_chain_len = u32::try_from(chain.len())
            .ok()
            .and_then(|len| len.checked_add(commitment_delay_limit))
            .unwrap_or_else(|| unreachable!("Not supported: height is out of u32"));

        debug_assert!(
            max_chain_len > 0,
            "Max chain length must be positive, because commitment delay limit is positive"
        );

        let last_prepared_block = chain
            .front()
            .expect("At least one block must be in chain if unknown committed announces was found")
            .header
            .parent_hash;
        let tail = calculate_announces_common_predecessor(cfg, last_prepared_block)?;

        let announces_request = AnnouncesRequest {
            head: announce_hash,
            tail: Some(tail),
            max_chain_len,
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
        MissingData {
            codes: missing_codes,
            required: RequiredData {
                codes: missing_validated_codes,
                announces,
            },
        },
        chain,
    ))
}

async fn prepare_one_block(
    cfg: &mut PrepareConfig<impl ProcessorExt>,
    block: SimpleBlockData,
) -> Result<()> {
    let parent = block.header.parent_hash;
    let mut requested_codes = HashSet::new();
    let mut validated_codes = HashSet::new();

    let parent_meta = cfg.db.block_meta(parent);
    let mut last_committed_batch = parent_meta
        .last_committed_batch
        .ok_or(ComputeError::LastCommittedBatchNotFound(parent))?;
    let mut codes_queue = parent_meta
        .codes_queue
        .ok_or(ComputeError::CodesQueueNotFound(parent))?;

    let mut last_committed_announce_hash = None;

    let events = cfg
        .db
        .block_events(block.hash)
        .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;
    for event in events {
        match event {
            BlockEvent::Router(RouterEvent::BatchCommitted { digest }) => {
                last_committed_batch = digest;
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

    let last_committed_announce_hash = if let Some(hash) = last_committed_announce_hash {
        chain_recovery_if_needed_and_check(cfg, &block, hash).await?;

        hash
    } else {
        parent_meta
            .last_committed_announce
            .ok_or(ComputeError::LastCommittedHeadNotFound(parent))?
    };

    // +_+_+ #4813 #4814 are fixed now
    let mut new_announces = vec![];
    for parent_announce_hash in parent_meta
        .announces
        .ok_or(ComputeError::AnnouncesNotFound(parent))?
    {
        if let Some(new_announce_hash) = propagate_from_parent_announce(
            cfg,
            block.hash,
            parent_announce_hash,
            last_committed_announce_hash,
        )
        .await?
        {
            new_announces.push(new_announce_hash);
        };
    }

    let last_announce = new_announces.last().cloned().ok_or_else(|| {
        log::error!("No announces could be propagated for block {}", block.hash);
        // This error could occur only if old not base announce was committed
        ConsensusGuaranteesError::CommitmentDelayLimitExceeded
    })?;

    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);

    cfg.db.mutate_block_meta(block.hash, |meta| {
        *meta = BlockMeta {
            prepared: true,
            announces: Some(new_announces),
            codes_queue: Some(codes_queue),
            last_committed_batch: Some(last_committed_batch),
            last_committed_announce: Some(last_committed_announce_hash),
        };
    });

    cfg.db
        .mutate_latest_data_if_some(|data| {
            data.prepared_block_hash = block.hash;
            data.computed_announce_hash = last_announce;
        })
        .ok_or(ComputeError::LatestDataNotFound)?;

    Ok(())
}

async fn chain_recovery_if_needed_and_check(
    cfg: &PrepareConfig<impl ProcessorExt>,
    block: &SimpleBlockData,
    last_committed_announce_hash: AnnounceHash,
) -> Result<()> {
    let &PrepareConfig {
        ref db,
        commitment_delay_limit,
        ..
    } = cfg;

    let last_committed_announce = db
        .announce(last_committed_announce_hash)
        .ok_or(ComputeError::AnnounceNotFound(last_committed_announce_hash))?;

    let last_committed_announce_height = db
        .block_header(last_committed_announce.block_hash)
        .ok_or(ComputeError::BlockHeaderNotFound(
            last_committed_announce.block_hash,
        ))?
        .height;

    let distance = block
        .header
        .height
        .checked_sub(last_committed_announce_height)
        .ok_or(ConsensusGuaranteesError::AnnounceFromFutureCommitted)?;

    if distance > commitment_delay_limit {
        // By T1 sequence 3 - if announce committed was committed before 
        if !last_committed_announce.is_base() {
            log::error!("Not base announce committed after commitment delay limit");
            Err(ConsensusGuaranteesError::CommitmentDelayLimitExceeded)?
        }
        if db.announce_meta(last_committed_announce_hash).computed {
            log::error!("Committed after commitment delay limit announce is not computed");
            Err(ConsensusGuaranteesError::CommitmentDelayLimitExceeded)?
        }

        // TODO +_+_+: append debug check, that last_committed_announce is a predecessor of any announce from `block.parent`
    } else {
        // In case distance between announce and block where announce is committed,
        // is smaller or equal to commitment delay limit,
        // then announce can be not base announce and can be not computed by this node.

        let mut not_computed_chain = VecDeque::new();
        let mut announce_hash = last_committed_announce_hash;
        let mut counter = 0;
        while !db.announce_meta(announce_hash).computed {
            counter += 1;
            if counter > commitment_delay_limit {
                log::error!(
                    "Chain of not computed announces is longer than commitment delay limit"
                );
                Err(ConsensusGuaranteesError::CommitmentDelayLimitExceeded)?;
            }

            let announce = db
                .announce(announce_hash)
                .ok_or(ComputeError::AnnounceNotFound(announce_hash))?;
            announce_hash = announce.parent;
            not_computed_chain.push_front(announce);
        }

        // Compute chain of not computed announces
        for announce in not_computed_chain {
            compute::compute(cfg.db.clone(), cfg.processor.clone(), announce).await?;
        }
    }

    Ok(())
}

/// Create a new base announce from provided parent announce hash.
/// Compute the announce and store related data in the database.
async fn propagate_from_parent_announce(
    cfg: &mut PrepareConfig<impl ProcessorExt>,
    block_hash: H256,
    parent_announce_hash: AnnounceHash,
    last_committed_announce_hash: AnnounceHash,
) -> Result<Option<AnnounceHash>> {
    let PrepareConfig {
        db,
        processor,
        commitment_delay_limit,
    } = cfg;

    // Check parent announce branch is not expired
    // The branch is expired if:
    // 1. It does not includes last committed announce
    // 2. If it includes not committed and not base announce,
    //    which is older than commitment delay limit:
    //    1) branch contains announce
    //    2) announce is not committed at block block_hash
    //    3) height(block_hash) - height(announce) >= commitment_delay_limit
    //    In this case announce can never be committed in all future blocks with predecessor `block_hash`
    //
    // We check here till commitment delay limit, because it can be proven that
    // any announce older than `commitment_delay_limit` and till `last_committed_announce`
    // is base announce, if blocks' preparation is done one by one from the oldest to the newest.
    let mut predecessor = parent_announce_hash;
    for i in 0..*commitment_delay_limit {
        if predecessor == last_committed_announce_hash {
            break;
        }

        let announce = db
            .announce(predecessor)
            .ok_or_else(|| ComputeError::AnnounceNotFound(predecessor))?;

        if i == *commitment_delay_limit + 1 && !announce.is_base() {
            // We reached the oldest announce in commitment delay limit which is not not committed yet.
            // This announce cannot be committed any more if it is not base announce,
            // so this branch as expired and have to skip propagation from `parent`.
            return Ok(None);
        }

        predecessor = announce.parent;
    }

    // TODO #4814: hack - use here base with gas to avoid unknown announces in tests,
    // this can be fixed by unknown announces handling later
    let new_base_announce = Announce::new_default_gas(block_hash, parent_announce_hash);
    let new_base_announce_hash = new_base_announce.hash();

    if db.announce_meta(new_base_announce_hash).computed {
        // One possible case is:
        // node execution was dropped before block was marked as prepared,
        // but announce was already marked as computed.
        // see also `announce_is_computed_and_included`
        log::warn!(
            "Announce {new_base_announce_hash:?} was already computed,
             means it was lost by some reasons, skip computation,
             but setting it as announce in block {block_hash:?}"
        );

        return Ok(Some(new_base_announce_hash));
    }

    let events = db
        .block_events(block_hash)
        .ok_or(ComputeError::BlockEventsNotFound(block_hash))?
        .into_iter()
        .filter_map(|event| event.to_request())
        .collect::<Vec<BlockRequestEvent>>();

    let BlockProcessingResult {
        transitions,
        states,
        schedule,
    } = processor
        .process_announce(new_base_announce.clone(), events)
        .await?;

    db.set_announce(new_base_announce);
    db.set_announce_outcome(new_base_announce_hash, transitions);
    db.set_announce_program_states(new_base_announce_hash, states);
    db.set_announce_schedule(new_base_announce_hash, schedule);
    db.mutate_announce_meta(new_base_announce_hash, |meta| {
        meta.computed = true;
    });

    Ok(Some(new_base_announce_hash))
}

/// Returns announce hash from T1 sequence 3 or global genesis/start announce
fn calculate_announces_common_predecessor(
    cfg: &PrepareConfig<impl ProcessorExt>,
    block_hash: H256,
) -> Result<AnnounceHash> {
    let PrepareConfig {
        db,
        commitment_delay_limit,
        ..
    } = cfg;

    let start_announce = db
        .latest_data()
        .ok_or(ComputeError::LatestDataNotFound)?
        .start_announce_hash;

    let mut announces = db
        .block_meta(block_hash)
        .announces
        .ok_or(ComputeError::AnnouncesNotFound(block_hash))?
        .into_iter()
        .collect::<HashSet<_>>();

    for _ in 0..*commitment_delay_limit {
        announces = announces
            .into_iter()
            .map(|announce_hash| {
                db.announce(announce_hash)
                    .map(|a| a.parent)
                    .ok_or(ComputeError::AnnounceNotFound(announce_hash))
            })
            .collect::<Result<HashSet<_>>>()?;

        if announces.len() == 1 && announces.contains(&start_announce) {
            return Ok(start_announce);
        }
    }

    if let Some(&announce) = announces.iter().next()
        && announces.len() == 1
    {
        return Ok(announce);
    }

    // If we reached here - common predecessor not found by some reasons
    Err(ConsensusGuaranteesError::Other(format!(
        "By some reasons common announces predecessor was not found for block {block_hash:?}"
    ))
    .into())
}

#[cfg(test)]
mod tests {
    use crate::tests::MockProcessor;

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
        db.set_announce_outcome(parent_announce.hash(), Default::default());
        db.set_announce_schedule(parent_announce.hash(), Default::default());
        db.set_announce_program_states(parent_announce.hash(), Default::default());

        db.mutate_block_meta(parent_hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some(vec![parent_announce.hash()]),
                codes_queue: Some(vec![code1_id].into()),
                last_committed_batch: Some(Digest::random()),
                last_committed_announce: Some(AnnounceHash::random()),
            }
        });
        db.set_validators(parent_hash, validators.clone());

        db.set_block_header(block.hash, block.header);

        db.mutate_latest_data(|data| *data = Some(Default::default()));

        let events = vec![
            BlockEvent::Router(RouterEvent::BatchCommitted {
                digest: batch_committed,
            }),
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(parent_announce.hash())),
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
        db.set_validators(block.hash, validators);
        db.set_block_synced(block.hash);

        // Prepare the block
        prepare_one_block(
            &mut PrepareConfig {
                db: db.clone(),
                processor: MockProcessor,
                commitment_delay_limit: 3,
            },
            block.clone(),
        )
        .await
        .unwrap();

        let meta = db.block_meta(block.hash);
        assert!(meta.prepared);
        assert_eq!(meta.codes_queue, Some(vec![code2_id].into()),);
        assert_eq!(meta.last_committed_batch, Some(batch_committed),);
        assert_eq!(meta.last_committed_announce, Some(parent_announce.hash()));
        assert_eq!(meta.announces.as_ref().map(|a| a.len()), Some(1));

        let announce_hash = meta.announces.unwrap()[0];
        let announce = db.announce(announce_hash).unwrap();
        assert_eq!(
            announce,
            Announce::new_default_gas(block.hash, parent_announce.hash())
        );
        assert!(db.announce_meta(announce_hash).computed);
        assert_eq!(db.announce_outcome(announce_hash), Some(Default::default()));
        assert_eq!(
            db.announce_schedule(announce_hash),
            Some(Default::default())
        );
        assert_eq!(
            db.announce_program_states(announce_hash),
            Some(Default::default())
        );
        assert_eq!(db.latest_data().unwrap().prepared_block_hash, block.hash);
    }
}
