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
    Announce, HashOf, SimpleBlockData,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, BlockMetaStorageRW,
        OnChainStorageRO,
    },
    network::{AnnouncesRequest, AnnouncesRequestUntil, CheckedAnnouncesResponse},
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
    state: WaitingFor,
}

#[derive(Debug)]
enum WaitingFor {
    ChainHead,
    SyncedBlock(SimpleBlockData),
    PreparedBlock(SimpleBlockData),
    MissingAnnounces {
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

        self.state = WaitingFor::SyncedBlock(block);

        Ok(self.into())
    }

    fn process_synced_block(mut self, block_hash: H256) -> Result<ValidatorState> {
        if let WaitingFor::SyncedBlock(block) = &self.state
            && block.hash == block_hash
        {
            self.state = WaitingFor::PreparedBlock(block.clone());

            Ok(self.into())
        } else {
            DefaultProcessing::synced_block(self, block_hash)
        }
    }

    fn process_prepared_block(mut self, block_hash: H256) -> Result<ValidatorState> {
        if let WaitingFor::PreparedBlock(block) = &self.state
            && block.hash == block_hash
        {
            let chain = self.ctx.core.collect_blocks_without_announces(block.hash)?;

            if let Some(first_block) = chain.front()
                && let Some(request) = self
                    .ctx
                    .core
                    .check_for_missing_announces(first_block.header.parent_hash)?
            {
                tracing::debug!(
                    "Missing announces detected for block {block_hash}, send request: {request:?}"
                );

                self.ctx.output(request);

                Ok(Self {
                    ctx: self.ctx,
                    state: WaitingFor::MissingAnnounces {
                        block: block.clone(),
                        chain,
                        announces: request,
                    },
                }
                .into())
            } else {
                self.ctx
                    .core
                    .propagate_announces(chain, Default::default())?;
                self.ctx.switch_to_producer_or_subordinate(block.clone())
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
            WaitingFor::MissingAnnounces {
                block,
                chain,
                announces,
            } if announces == *response.request() => {
                tracing::debug!(block = %block.hash, "Received missing announces response");

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
            state: WaitingFor::ChainHead,
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
            .block_validators(block.hash)
            .ok_or(anyhow!("validators not found for block({})", block.hash))?;

        let producer = utils::block_producer_for(
            &validators,
            block.header.timestamp,
            self.core.slot_duration.as_secs(),
        );
        let my_address = self.core.pub_key.to_address();

        if my_address == producer {
            tracing::info!(block = %block.hash, "👷 Start to work as a producer");

            Producer::create(self, block, validators.clone())
        } else {
            // TODO #4636: add test (in ethexe-service) for case where is not validator for current block
            let is_validator_for_current_block = validators.contains(&my_address);

            tracing::info!(
                block = %block.hash,
                "👷 Start to work as subordinate, producer is {producer}, \
                I'm validator for current block: {is_validator_for_current_block}",
            );

            Subordinate::create(self, block, producer, is_validator_for_current_block)
        }
    }
}

impl ValidatorCore {
    fn collect_blocks_without_announces(
        &self,
        starting_block: H256,
    ) -> Result<VecDeque<SimpleBlockData>> {
        let mut blocks = VecDeque::new();
        let mut current_block = starting_block;

        loop {
            let header = self
                .db
                .block_header(current_block)
                .ok_or_else(|| anyhow!("header not found for block({current_block})"))?;

            if self.db.block_meta(current_block).announces.is_some() {
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

    fn propagate_announces(
        &self,
        chain: VecDeque<SimpleBlockData>,
        mut missing_announces: HashMap<HashOf<Announce>, Announce>,
    ) -> Result<()> {
        // iterate over the collected blocks from oldest to newest and propagate announces
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
        last_committed_announce_hash: HashOf<Announce>,
        missing_announces: &mut HashMap<HashOf<Announce>, Announce>,
    ) -> Result<()> {
        let mut announce_hash = last_committed_announce_hash;
        while !self.announce_is_included(announce_hash) {
            tracing::debug!(announce = %announce_hash, "Committed announces was not included yet, including...");

            let announce = missing_announces.remove(&announce_hash).ok_or_else(|| {
                anyhow!("Committed announce {announce_hash} not found in missing announces")
            })?;

            announce_hash = announce.parent;

            self.include_announce(announce)?;
        }

        Ok(())
    }

    /// Create a new base announce from provided parent announce hash.
    /// Compute the announce and store related data in the database.
    fn propagate_one_base_announce(
        &self,
        block_hash: H256,
        parent_announce_hash: HashOf<Announce>,
        last_committed_announce_hash: HashOf<Announce>,
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
                tracing::trace!(
                    predecessor = %predecessor,
                    parent_announce = %parent_announce_hash,
                    "predecessor is too old and not base, so parent announce branch is expired",
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

        tracing::trace!(
            parent_announce = %parent_announce_hash,
            new_base_announce = %new_base_announce.to_hash(),
            "branch from parent announce is not expired, propagating new base announce",
        );

        self.include_announce(new_base_announce)?;

        Ok(())
    }

    fn check_for_missing_announces(
        &self,
        last_with_announces_block_hash: H256,
    ) -> Result<Option<AnnouncesRequest>> {
        let last_committed_announce_hash = self
            .db
            .block_meta(last_with_announces_block_hash)
            .last_committed_announce
            .ok_or_else(|| {
                anyhow!(
                    "last committed announce not found for block {last_with_announces_block_hash}",
                )
            })?;

        if self.announce_is_included(last_committed_announce_hash) {
            // announce is already included, no need to request announces
            // +_+_+ debug check if all announces in the chain are present
            Ok(None)
        } else {
            // announce is unknown, or not included, so there can be missing announces
            // and node needs to request all announces till definitely known one
            let common_predecessor_announce_hash =
                self.find_announces_common_predecessor(last_with_announces_block_hash)?;

            Ok(Some(AnnouncesRequest {
                head: last_committed_announce_hash,
                until: AnnouncesRequestUntil::Tail(common_predecessor_announce_hash),
            }))
        }
    }

    pub fn announce_is_included(&self, announce_hash: HashOf<Announce>) -> bool {
        // Consider zero announce hash as always included
        if announce_hash == HashOf::zero() {
            return true;
        }

        self.db
            .announce(announce_hash)
            .and_then(|announce| self.db.block_meta(announce.block_hash).announces)
            .map(|announces| announces.contains(&announce_hash))
            .unwrap_or(false)
    }

    pub fn include_announce(&self, announce: Announce) -> Result<HashOf<Announce>> {
        tracing::trace!(announce = %announce.to_hash(), "Including announce");

        let block_hash = announce.block_hash;
        let announce_hash = self.db.set_announce(announce);

        let mut not_yet_included = true;
        self.db.mutate_block_meta(block_hash, |meta| {
            not_yet_included = meta.announces.get_or_insert_default().insert(announce_hash);
        });

        not_yet_included.then_some(announce_hash).ok_or_else(|| {
            anyhow!("announce {announce_hash} for block {block_hash} was already included")
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{ConsensusEvent, validator::mock::*};
    use ethexe_common::{mock::*, network::AnnouncesResponse};
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
        gear_utils::init_default_logger();

        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![
            ctx.core.pub_key.to_address(),
            keys[0].to_address(),
            keys[1].to_address(),
        ]
        .into();

        let block = BlockChain::mock((2, validators)).setup(&ctx.core.db).blocks[2].to_simple();

        let state = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let state = state.process_synced_block(block.hash).unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let state = state.process_prepared_block(block.hash).unwrap();
        assert!(state.is_producer(), "got {:?}", state);
    }

    #[test]
    fn switch_to_subordinate() {
        gear_utils::init_default_logger();

        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![
            ctx.core.pub_key.to_address(),
            keys[1].to_address(),
            keys[2].to_address(),
        ]
        .into();

        let block = BlockChain::mock((1, validators)).setup(&ctx.core.db).blocks[1].to_simple();

        let state = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let state = state.process_synced_block(block.hash).unwrap();
        assert!(state.is_initial(), "expected Initial, got {:?}", state);

        let state = state.process_prepared_block(block.hash).unwrap();
        assert!(
            state.is_subordinate(),
            "expected Subordinate, got {:?}",
            state
        );
    }

    #[test]
    fn missing_announces_request_response() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let last = 9;

        let mut chain = BlockChain::mock(last as u32);
        chain.blocks[last].as_prepared_mut().announces = None;

        // create 2 missing announces from blocks last - 2 and last - 1
        let announce8 = Announce::with_default_gas(
            chain.blocks[last - 2].hash,
            chain.block_top_announce_hash(last - 3),
        );
        let announce9 =
            Announce::with_default_gas(chain.blocks[last - 1].hash, announce8.to_hash());

        chain.blocks[last].as_prepared_mut().last_committed_announce = announce9.to_hash();
        let chain = chain.setup(&ctx.core.db);
        let block = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, block.clone())
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();
        assert!(state.is_initial(), "got {:?}", state);
        let expected_request = AnnouncesRequest {
            head: chain.blocks[last].as_prepared().last_committed_announce,
            until: chain.block_top_announce_hash(last - 3).into(),
        };
        assert_eq!(state.context().output, vec![expected_request.into()]);

        let response = AnnouncesResponse {
            announces: vec![
                chain
                    .announces
                    .get(&chain.block_top_announce_hash(last - 3))
                    .unwrap()
                    .announce
                    .clone(),
                announce8.clone(),
                announce9.clone(),
            ],
        }
        .try_into_checked(expected_request)
        .unwrap();

        // In successful case no new events are produced
        let state = state.process_announces_response(response).unwrap();
        assert_eq!(state.context().output, vec![expected_request.into()]);
    }

    #[test]
    fn announce_propagation_done() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let last = 9;
        let chain = BlockChain::mock(last as u32)
            .tap_mut(|chain| {
                // remove announces from 5 latest blocks
                (last - 4..=last).for_each(|idx| {
                    chain.blocks[idx].as_prepared_mut().announces = None;
                });

                // append one more announce to the block last - 5
                let announce = Announce::with_default_gas(
                    chain.blocks[last - 5].hash,
                    chain.block_top_announce_hash(last - 6),
                );
                chain.blocks[last - 5]
                    .as_prepared_mut()
                    .announces
                    .as_mut()
                    .unwrap()
                    .insert(announce.to_hash());
                chain.announces.insert(
                    announce.to_hash(),
                    AnnounceData {
                        announce,
                        computed: None,
                    },
                );
            })
            .setup(&ctx.core.db);
        let block = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, block.clone())
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();

        let ctx = state.into_context();
        assert_eq!(ctx.output, vec![]);
        for i in last - 5..last - 5 + ctx.core.commitment_delay_limit as usize {
            let announces = ctx.core.db.block_meta(chain.blocks[i].hash).announces;
            assert_eq!(announces.unwrap().len(), 2);
        }
        for i in last - 5 + ctx.core.commitment_delay_limit as usize..=last {
            let announces = ctx.core.db.block_meta(chain.blocks[i].hash).announces;
            assert_eq!(announces.unwrap().len(), 1);
        }
    }

    #[test]
    fn announce_propagation_many_missing_blocks() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let last = 12;
        let chain = BlockChain::mock(last as u32)
            .tap_mut(|chain| {
                // remove announces from 10 latest blocks
                (last - 9..=last).for_each(|idx| {
                    chain.blocks[idx].as_prepared_mut().announces = None;
                });
            })
            .setup(&ctx.core.db);
        let head = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, head.clone())
            .unwrap()
            .process_synced_block(head.hash)
            .unwrap()
            .process_prepared_block(head.hash)
            .unwrap();

        let ctx = state.into_context();
        assert_eq!(ctx.output, vec![]);
        (last - 9..=last).for_each(|idx| {
            let block_hash = chain.blocks[idx].hash;
            let announces = ctx.core.db.block_meta(block_hash).announces;
            assert!(
                announces.is_some(),
                "expected announces to be propagated for block {block_hash}"
            );
            assert_eq!(
                announces.unwrap().len(),
                1,
                "unexpected announces count for block {block_hash}"
            );
        });
    }

    #[test]
    fn process_synced_block_rejected() {
        gear_utils::init_default_logger();

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

    #[test]
    fn process_prepared_block_rejected() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let state = Initial::create_with_chain_head(ctx, block.clone())
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(H256::random())
            .unwrap();
        assert!(state.is_initial(), "got {:?}", state);
        assert_eq!(state.context().output.len(), 1);
        assert!(matches!(
            state.context().output[0],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn process_announces_response_rejected() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let block = BlockChain::mock(1)
            .setup(&ctx.core.db)
            .tap_mut(|chain| {
                chain.blocks[1].as_prepared_mut().announces = None;
                chain.blocks[1].as_prepared_mut().last_committed_announce = HashOf::random();
            })
            .setup(&ctx.core.db)
            .blocks[1]
            .to_simple();

        let invalid_announce = Announce {
            block_hash: H256::random(),
            parent: HashOf::random(),
            gas_allowance: None,
            off_chain_transactions: vec![],
        };
        let invalid_announce_hash = invalid_announce.to_hash();

        let state = Initial::create_with_chain_head(ctx, block.clone())
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap()
            .process_announces_response(
                AnnouncesResponse {
                    announces: vec![invalid_announce],
                }
                .try_into_checked(AnnouncesRequest {
                    head: invalid_announce_hash,
                    until: NonZeroU32::new(1).unwrap().into(),
                })
                .unwrap(),
            )
            .unwrap();
        assert!(state.is_initial(), "got {:?}", state);
        assert_eq!(state.context().output.len(), 2);
        assert!(matches!(
            state.context().output[1],
            ConsensusEvent::Warning(_)
        ));
    }
}
