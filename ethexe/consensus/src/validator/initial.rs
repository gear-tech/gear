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
use crate::{
    announces::{self, DBAnnouncesExt},
    utils,
};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    SimpleBlockData,
    db::OnChainStorageRO,
    network::{AnnouncesRequest, AnnouncesResponse},
};
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

/// State transition flow:
///
/// ```text
/// ChainHead (waiting for new chain head)
///   |
///   ├─ receive new chain head
///   |
/// SyncedBlock (waiting block is synced)
///   |
///   ├─ receive block is synced
///   |
/// PreparedBlock (waiting block is prepared)
///   |
///   ├─ receive block is prepared
///   |
///   └─ check for missing announces
///     |
///     ├─ if any missing announces
///     |   |
///     |  MissingAnnounces (waiting for requested missing announces from network)
///     |   |
///     |   └─ receive announces response, then do propagation
///     |       ├─ if is producer ─► Producer
///     |       └─ if is subordinate ─► Subordinate
///     |
///     └─ if no missing, then do propagation
///         ├─ if is producer ─► Producer
///         └─ if is subordinate ─► Subordinate
/// ```
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
            self.state = WaitingFor::PreparedBlock(*block);

            Ok(self.into())
        } else {
            DefaultProcessing::synced_block(self, block_hash)
        }
    }

    fn process_prepared_block(mut self, block_hash: H256) -> Result<ValidatorState> {
        if let WaitingFor::PreparedBlock(block) = &self.state
            && block.hash == block_hash
        {
            let chain = self
                .ctx
                .core
                .db
                .collect_blocks_without_announces(block_hash)?;

            tracing::trace!(block = %block.hash, "Collected blocks without announces: {chain:?}");

            if let Some(first_block) = chain.front()
                && let Some(request) = announces::check_for_missing_announces(
                    &self.ctx.core.db,
                    block_hash,
                    first_block.header.parent_hash,
                    self.ctx.core.commitment_delay_limit,
                )?
            {
                tracing::debug!(
                    "Missing announces detected for block {block_hash}, send request: {request:?}"
                );

                self.ctx.output(request);

                Ok(Self {
                    ctx: self.ctx,
                    state: WaitingFor::MissingAnnounces {
                        block: *block,
                        chain,
                        announces: request,
                    },
                }
                .into())
            } else {
                tracing::debug!(block = %block.hash, "No missing announces");

                announces::propagate_announces(
                    &self.ctx.core.db,
                    chain,
                    self.ctx.core.commitment_delay_limit,
                    Default::default(),
                )?;
                self.ctx.replay_rejected_announces(block.hash)?;

                self.ctx.switch_to_producer_or_subordinate(*block)
            }
        } else {
            DefaultProcessing::prepared_block(self, block_hash)
        }
    }

    fn process_announces_response(mut self, response: AnnouncesResponse) -> Result<ValidatorState> {
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

                announces::propagate_announces(
                    &self.ctx.core.db,
                    chain,
                    self.ctx.core.commitment_delay_limit,
                    missing_announces,
                )?;
                self.ctx.replay_rejected_announces(block.hash)?;

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
        let era_index = self.core.timelines.era_from_ts(block.header.timestamp);
        let validators = self
            .core
            .db
            .validators(era_index)
            .ok_or(anyhow!("validators not found for era {era_index}"))?;

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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{ConsensusEvent, announces::AnnounceStatus, validator::mock::*};
    use ethexe_common::{
        Announce, HashOf, ValidatorsVec, db::*, mock::*, network::AnnouncesResponse,
    };
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

        let (mut ctx, keys, _) = mock_validator_context();
        let validators: ValidatorsVec = nonempty![
            ctx.core.pub_key.to_address(),
            keys[0].to_address(),
            keys[1].to_address(),
        ]
        .into();

        let chain = BlockChain::mock((2, validators)).setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let block = chain.blocks[2].to_simple();

        let state = Initial::create_with_chain_head(ctx, block).unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let state = state.process_synced_block(block.hash).unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let state = state.process_prepared_block(block.hash).unwrap();
        assert!(state.is_producer(), "got {:?}", state);
    }

    #[test]
    fn switch_to_subordinate() {
        gear_utils::init_default_logger();

        let (mut ctx, keys, _) = mock_validator_context();
        let validators: ValidatorsVec = nonempty![
            ctx.core.pub_key.to_address(),
            keys[1].to_address(),
            keys[2].to_address(),
        ]
        .into();

        let chain = BlockChain::mock((1, validators)).setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let block = chain.blocks[1].to_simple();
        let state = Initial::create_with_chain_head(ctx, block).unwrap();
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

        let (mut ctx, _, _) = mock_validator_context();
        let last = 9;

        let mut chain = BlockChain::mock(last as u32);
        chain.blocks[last].as_prepared_mut().announces = None;

        // create 2 missing announces from blocks last - 2 and last - 1
        let announce2 = Announce::with_default_gas(
            chain.blocks[last - 2].hash,
            chain.block_top_announce_hash(last - 3),
        );
        let announce1 =
            Announce::with_default_gas(chain.blocks[last - 1].hash, announce2.to_hash());

        chain.blocks[last].as_prepared_mut().last_committed_announce = announce1.to_hash();
        let chain = chain.setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let block = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, block)
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();
        assert!(state.is_initial(), "got {:?}", state);

        let tail = chain.block_top_announce_hash(last - 4);
        let expected_request = AnnouncesRequest {
            head: chain.blocks[last].as_prepared().last_committed_announce,
            until: tail.into(),
        };
        assert_eq!(state.context().output, vec![expected_request.into()]);

        let response = unsafe {
            AnnouncesResponse::from_parts(
                expected_request,
                vec![
                    chain
                        .announces
                        .get(&chain.block_top_announce_hash(last - 3))
                        .unwrap()
                        .announce
                        .clone(),
                    announce2.clone(),
                    announce1.clone(),
                ],
            )
        };

        // In successful case no new events are produced
        let state = state.process_announces_response(response).unwrap();
        assert_eq!(state.context().output, vec![expected_request.into()]);
    }

    #[test]
    fn announce_propagation_done() {
        gear_utils::init_default_logger();

        let (mut ctx, _, _) = mock_validator_context();
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
        ctx.core.timelines = chain.protocol_timelines;
        let block = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, block)
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

        let (mut ctx, _, _) = mock_validator_context();
        let last = 12;
        let chain = BlockChain::mock(last as u32)
            .tap_mut(|chain| {
                // remove announces from 10 latest blocks
                (last - 9..=last).for_each(|idx| {
                    chain.blocks[idx].as_prepared_mut().announces = None;
                });
            })
            .setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let head = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, head)
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
    fn replay_rejected_chain_after_propagation() {
        gear_utils::init_default_logger();

        let (mut ctx, _, _) = mock_validator_context();
        let last = 5;
        let chain = BlockChain::mock(last as u32).setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let head = chain.blocks[last].to_simple();

        let announce3 = Announce::with_default_gas(
            chain.blocks[last - 2].hash,
            chain.block_top_announce_hash(last - 3),
        );
        let announce4 =
            Announce::with_default_gas(chain.blocks[last - 1].hash, announce3.to_hash());
        let announce5 = Announce::with_default_gas(chain.blocks[last].hash, announce4.to_hash());

        let announce4_hash = announce4.to_hash();
        let announce5_hash = announce5.to_hash();

        ctx.rejected_announces
            .push(announce4_hash, announce4.clone());
        ctx.rejected_announces
            .push(announce5_hash, announce5.clone());

        assert!(matches!(
            announces::accept_announce(&ctx.core.db, announce3).unwrap(),
            AnnounceStatus::Accepted(_),
        ));

        let state = Initial::create_with_chain_head(ctx, head)
            .unwrap()
            .process_synced_block(head.hash)
            .unwrap()
            .process_prepared_block(head.hash)
            .unwrap();

        let output = &state.context().output;
        assert!(output.iter().any(|event| {
            matches!(
                event,
                ConsensusEvent::AnnounceAccepted(hash) if *hash == announce4_hash
            )
        }));
        assert!(output.iter().any(|event| {
            matches!(
                event,
                ConsensusEvent::ComputeAnnounce(announce) if announce.to_hash() == announce4_hash
            )
        }));
        assert!(output.iter().any(|event| {
            matches!(
                event,
                ConsensusEvent::AnnounceAccepted(hash) if *hash == announce5_hash
            )
        }));
        assert!(output.iter().any(|event| {
            matches!(
                event,
                ConsensusEvent::ComputeAnnounce(announce) if announce.to_hash() == announce5_hash
            )
        }));
        assert!(
            state
                .context()
                .rejected_announces
                .peek(&announce4_hash)
                .is_none()
        );
        assert!(
            state
                .context()
                .rejected_announces
                .peek(&announce5_hash)
                .is_none()
        );
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
        let state = Initial::create_with_chain_head(ctx, block)
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

        let invalid_announce = Announce::base(H256::random(), HashOf::random());
        let invalid_announce_hash = invalid_announce.to_hash();

        let response = unsafe {
            AnnouncesResponse::from_parts(
                AnnouncesRequest {
                    head: invalid_announce_hash,
                    until: NonZeroU32::new(1).unwrap().into(),
                },
                vec![invalid_announce],
            )
        };

        let state = Initial::create_with_chain_head(ctx, block)
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap()
            .process_announces_response(response)
            .unwrap();
        assert!(state.is_initial(), "got {:?}", state);
        assert_eq!(state.context().output.len(), 2);
        assert!(matches!(
            state.context().output[1],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn commitment_with_delay() {
        gear_utils::init_default_logger();

        let (mut ctx, _, _) = mock_validator_context();
        let last = 10;
        let mut chain = BlockChain::mock(last as u32);

        // create unknown announce for block last - 6
        let unknown_announce = Announce::with_default_gas(
            chain.blocks[last - 6].hash,
            chain.block_top_announce_hash(last - 7),
        );
        let unknown_announce_hash = unknown_announce.to_hash();

        // remove announces from 5 latest blocks
        for idx in last - 4..=last {
            chain.blocks[idx]
                .as_prepared_mut()
                .announces
                .iter()
                .flatten()
                .for_each(|ah| {
                    chain.announces.remove(ah);
                });
            chain.blocks[idx].as_prepared_mut().announces = None;

            // set unknown_announce as last committed announce
            chain.blocks[idx].as_prepared_mut().last_committed_announce = unknown_announce_hash;
        }

        let chain = chain.setup(&ctx.core.db);
        ctx.core.timelines = chain.protocol_timelines;
        let block = chain.blocks[last].to_simple();

        let state = Initial::create_with_chain_head(ctx, block)
            .unwrap()
            .process_synced_block(block.hash)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();

        assert!(state.is_initial(), "got {:?}", state);

        let expected_request = AnnouncesRequest {
            head: chain.blocks[last].as_prepared().last_committed_announce,
            until: chain.block_top_announce_hash(last - 8).into(),
        };
        assert_eq!(state.context().output, vec![expected_request.into()]);

        let response = unsafe {
            AnnouncesResponse::from_parts(
                expected_request,
                vec![
                    chain
                        .announces
                        .get(&chain.block_top_announce_hash(last - 7))
                        .unwrap()
                        .announce
                        .clone(),
                    unknown_announce,
                ],
            )
        };

        let state = state.process_announces_response(response).unwrap();
        assert!(state.is_subordinate(), "got {:?}", state);
        assert_eq!(
            state.context().output.len(),
            1,
            "No additional output expected, got {:?}",
            state.context().output
        );
    }
}
