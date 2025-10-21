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

use std::collections::VecDeque;

use super::{
    DefaultProcessing, StateHandler, ValidatorContext, ValidatorState, producer::Producer,
    subordinate::Subordinate,
};
use crate::{utils, validator::core::ValidatorCore};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Announce, AnnounceHash, AnnouncesRequest, AnnouncesRequestUntil, CheckedAnnouncesResponse,
    SimpleBlockData,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMetaStorageRead, BlockMetaStorageWrite,
        OnChainStorageRead,
    },
    events::{BlockEvent, RouterEvent},
};
use ethexe_ethereum::primitives::map::HashMap;
use gprimitives::H256;

/// [`Initial`] is the first state of the validator.
/// It waits for the chain head and this block on-chain information sync.
/// After block is fully synced it switches to either [`Producer`] or [`Subordinate`].
#[derive(Debug, Display)]
#[display("INITIAL in {:?}", self.state)]
pub struct Initial {
    ctx: ValidatorContext,
    state: State,
}

#[derive(Debug)]
enum State {
    WaitingForChainHead,
    WaitingForSyncedBlock(SimpleBlockData),
    WaitingForPreparedBlock(SimpleBlockData),
    WaitingForMissingAnnounces {
        block: SimpleBlockData,
        chain: VecDeque<SimpleBlockData>,
        announces: AnnouncesRequest,
    },
}

impl StateHandler for Initial {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_new_head(mut self, block: SimpleBlockData) -> Result<ValidatorState> {
        // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.

        self.state = State::WaitingForSyncedBlock(block);

        Ok(self.into())
    }

    fn process_synced_block(mut self, block_hash: H256) -> Result<ValidatorState> {
        if let State::WaitingForSyncedBlock(block) = &self.state
            && block.hash == block_hash
        {
            self.state = State::WaitingForPreparedBlock(block.clone());

            Ok(self.into())
        } else {
            DefaultProcessing::synced_block(self, block_hash)
        }
    }

    fn process_prepared_block(self, block_hash: H256) -> Result<ValidatorState> {
        if let State::WaitingForPreparedBlock(block) = &self.state
            && block.hash == block_hash
        {
            match self.ctx.core.search_for_missing_announces(block.hash)? {
                (Some(request), chain) => {
                    log::debug!(
                        "Missing announces detected for block {block_hash}, send request: {request:?}"
                    );

                    Ok(Self {
                        ctx: self.ctx,
                        state: State::WaitingForMissingAnnounces {
                            block: block.clone(),
                            chain,
                            announces: request,
                        },
                    }
                    .into())
                }
                (None, chain) => {
                    let block = block.clone();
                    self.ctx
                        .core
                        .propagate_announces(chain, Default::default())?;
                    self.ctx.switch_to_producer_or_subordinate(block)
                }
            }
        } else {
            DefaultProcessing::prepared_block(self, block_hash)
        }
    }

    fn process_announces_response(
        mut self,
        response: CheckedAnnouncesResponse,
    ) -> Result<ValidatorState> {
        match self.state {
            State::WaitingForMissingAnnounces {
                block,
                chain,
                announces,
            } if announces == *response.request() => {
                log::debug!("Received announces response for block {}", block.hash);

                let missing_announces = response
                    .into_parts()
                    .1
                    .into_iter()
                    .map(|a| (a.to_hash(), a))
                    .collect();
                self.ctx
                    .core
                    .propagate_announces(chain, missing_announces)?;
                self.ctx.switch_to_producer_or_subordinate(block)
            }
            state => {
                self.state = state;
                DefaultProcessing::announces_response(self, response)
            }
        }
    }
}

impl Initial {
    pub fn create(ctx: ValidatorContext) -> Result<ValidatorState> {
        Ok(Self {
            ctx,
            state: State::WaitingForChainHead,
        }
        .into())
    }

    pub fn create_with_chain_head(
        ctx: ValidatorContext,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        Self::create(ctx)?.process_new_head(block)
    }
}

impl ValidatorContext {
    fn switch_to_producer_or_subordinate(self, block: SimpleBlockData) -> Result<ValidatorState> {
        let validators = self
            .core
            .db
            .validators(block.hash)
            .ok_or(anyhow!("validators not found for block({})", block.hash))?;

        let producer = utils::block_producer_for(
            &validators,
            block.header.timestamp,
            self.core.slot_duration.as_secs(),
        );
        let my_address = self.core.pub_key.to_address();

        if my_address == producer {
            log::info!("👷 Start to work as a producer for block: {}", block.hash);

            Producer::create(self, block, validators.clone())
        } else {
            // TODO #4636: add test (in ethexe-service) for case where is not validator for current block
            let is_validator_for_current_block = validators.contains(&my_address);

            log::info!(
                "👷 Start to work as a subordinate for block: {}, producer is {producer}, \
                I'm validator for current block: {is_validator_for_current_block}",
                block.hash
            );

            Subordinate::create(self, block, producer, is_validator_for_current_block)
        }
    }
}

impl ValidatorCore {
    fn propagate_announces(
        &self,
        chain: VecDeque<SimpleBlockData>,
        mut missing_announces: HashMap<AnnounceHash, Announce>,
    ) -> Result<()> {
        for block in chain {
            debug_assert!(
                self.db.block_meta(block.hash).announces.is_none(),
                "Block {} should not have announces propagated yet",
                block.hash
            );

            let last_committed_announce_hash = self
                .db
                .block_meta(block.hash)
                .last_committed_announce
                .ok_or_else(|| {
                    anyhow!(
                        "Last committed announce hash not found for prepared block({})",
                        block.hash
                    )
                })?;

            self.announces_chain_recovery_if_needed(
                last_committed_announce_hash,
                &mut missing_announces,
            )?;

            for parent_announce_hash in self
                .db
                .block_meta(block.header.parent_hash)
                .announces
                .ok_or_else(|| {
                    anyhow!(
                        "Parent block({}) announces are missing",
                        block.header.parent_hash
                    )
                })?
            {
                self.propagate_one_base_announce(
                    block.hash,
                    parent_announce_hash,
                    last_committed_announce_hash,
                )?;
            }
        }

        Ok(())
    }

    fn announces_chain_recovery_if_needed(
        &self,
        last_committed_announce_hash: AnnounceHash,
        missing_announces: &mut HashMap<AnnounceHash, Announce>,
    ) -> Result<()> {
        let mut announce_hash = last_committed_announce_hash;
        while !self.announce_is_included(announce_hash) {
            log::debug!("Committed announces {announce_hash} was not included yet, including...");

            let announce = missing_announces.remove(&announce_hash).ok_or_else(|| {
                anyhow!("Committed announce {announce_hash} not found in missing announces")
            })?;

            announce_hash = announce.parent;

            self.include_announce(announce);
        }

        Ok(())
    }

    /// Create a new base announce from provided parent announce hash.
    /// Compute the announce and store related data in the database.
    fn propagate_one_base_announce(
        &self,
        block_hash: H256,
        parent_announce_hash: AnnounceHash,
        last_committed_announce_hash: AnnounceHash,
    ) -> Result<()> {
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
                .ok_or_else(|| anyhow!("announce({predecessor}) not found"))?;

            if i == self.commitment_delay_limit - 1 && !predecessor_announce.is_base() {
                // We reached the oldest announce in commitment delay limit which is not not committed yet.
                // This announce cannot be committed any more if it is not base announce,
                // so this branch is expired and we have to skip propagation from `parent`.
                log::trace!(
                    "predecessor {predecessor} is too old and not base, so {parent_announce_hash} branch is expired",
                );
                return Ok(());
            }

            // Check neighbor announces to be last committed announce
            if self
                .db
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
                return Ok(());
            };

            predecessor = predecessor_announce.parent;
        }

        let new_base_announce = Announce::base(block_hash, parent_announce_hash);

        log::trace!(
            "branch from {parent_announce_hash} is not expired, new announce {new_base_announce:?}"
        );

        self.include_announce(new_base_announce);

        Ok(())
    }

    fn search_for_missing_announces(
        &self,
        block_hash: H256,
    ) -> Result<(Option<AnnouncesRequest>, VecDeque<SimpleBlockData>)> {
        // collect blocks without announces propagated
        // find for the last committed announce in the chain
        let mut chain = VecDeque::new();
        let mut last_committed_announce = None;
        let mut current_block = block_hash;
        loop {
            let announces = self.db.block_meta(current_block).announces;

            if let Some(announces) = announces {
                if announces.is_empty() {
                    return Err(anyhow!("{current_block} has empty announces list"));
                }

                break;
            }

            self.db
                .block_events(current_block)
                .ok_or_else(|| anyhow!("events not found for {current_block}"))?
                .into_iter()
                .filter_map(|event| {
                    if let BlockEvent::Router(RouterEvent::AnnouncesCommitted(head)) = event {
                        Some(head)
                    } else {
                        None
                    }
                })
                .last()
                .map(|hash| last_committed_announce.get_or_insert(hash));

            let header = self
                .db
                .block_header(current_block)
                .ok_or(anyhow!("header not found for block({current_block})"))?;

            chain.push_front(SimpleBlockData {
                hash: current_block,
                header,
            });
            current_block = header.parent_hash;
        }

        let Some(announce_hash) = last_committed_announce else {
            // no announces were committed yet
            return Ok((None, chain));
        };

        if let Some(announce) = self.db.announce(announce_hash)
            && let Some(announces) = self.db.block_meta(announce.block_hash).announces
            && announces.contains(&announce_hash)
        {
            // announce is already included, no need to request announces

            // +_+_+ debug check if all announces in the chain are present

            Ok((None, chain))
        } else {
            // announce is unknown, or not included, so there can be missing announces
            // and we need to request all chain of announces

            let first_not_propagated_block = chain
                .front()
                .cloned()
                .expect("If at least on announce is committed, chain cannot be empty");

            let common_predecessor_announce_hash = self
                .find_announces_common_predecessor(first_not_propagated_block.header.parent_hash)?;

            Ok((
                Some(AnnouncesRequest {
                    head: announce_hash,
                    until: AnnouncesRequestUntil::Tail(common_predecessor_announce_hash),
                }),
                chain,
            ))
        }
    }

    fn announce_is_included(&self, announce_hash: AnnounceHash) -> bool {
        self.db
            .announce(announce_hash)
            .and_then(|announce| self.db.block_meta(announce.block_hash).announces)
            .map(|announces| announces.contains(&announce_hash))
            .unwrap_or(false)
    }

    fn include_announce(&self, announce: Announce) {
        let block_hash = announce.block_hash;
        let announce_hash = self.db.set_announce(announce);
        self.db.mutate_block_meta(block_hash, |meta| {
            meta.announces.get_or_insert_default().insert(announce_hash);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConsensusEvent, validator::mock::*};
    use ethexe_common::{db::*, mock::*};
    use gprimitives::H256;
    use nonempty::nonempty;

    #[test]
    fn create_initial_success() {
        let (ctx, _, _) = mock_validator_context();
        let initial = Initial::create(ctx).unwrap();
        assert!(initial.is_initial());
    }

    #[test]
    fn create_with_chain_head_success() {
        let (ctx, _, _) = mock_validator_context();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let initial = Initial::create_with_chain_head(ctx, block).unwrap();
        assert!(initial.is_initial());
    }

    #[tokio::test]
    async fn switch_to_producer() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![
            ctx.core.pub_key.to_address(),
            keys[0].to_address(),
            keys[1].to_address(),
        ];

        let block = BlockChain::mock(2).setup(&ctx.core.db).blocks[2].to_simple();

        ctx.core
            .db
            .set_block_validators(block.hash, validators.clone());

        let state = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        assert!(state.is_initial(), "expected Initial, got {:?}", state);

        let state = state.process_synced_block(block.hash).unwrap();
        assert!(state.is_initial(), "expected Initial, got {:?}", state);

        let state = state.process_prepared_block(block.hash).unwrap();
        assert!(state.is_producer(), "expected Producer, got {:?}", state);
    }

    #[test]
    fn switch_to_subordinate() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![
            ctx.core.pub_key.to_address(),
            keys[1].to_address(),
            keys[2].to_address(),
        ];

        let block = BlockChain::mock((1, validators)).setup(&ctx.core.db).blocks[1].to_simple();

        let state = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        assert!(state.is_initial(), "expected Initial, got {:?}", state);

        let state = state.process_synced_block(block.hash).unwrap();
        assert!(state.is_initial(), "expected Initial, got {:?}", state);

        let state = state.process_prepared_block(block.hash).unwrap();
        assert!(
            state.is_subordinate(),
            "expected Subordinate, got {:?}",
            state
        );
    }

    // +_+_+ make a test for missing announces request/response
    // +_+_+ make a test for announce propagation done

    #[test]
    fn process_synced_block_rejected() {
        let (ctx, _, _) = mock_validator_context();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();

        let initial = Initial::create(ctx)
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap();
        assert!(initial.is_initial());
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));

        let random_block = H256::random();
        let initial = initial
            .process_new_head(block)
            .unwrap()
            .process_synced_block(random_block)
            .unwrap();
        assert!(initial.is_initial());
        assert!(matches!(
            initial.context().output[1],
            ConsensusEvent::Warning(_)
        ));
    }

    // +_+_+ make a test prepared block rejected
}
