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

use super::{
    DefaultProcessing, StateHandler, ValidatorContext, ValidatorState, producer::Producer,
    subordinate::Subordinate,
};
use crate::utils;
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{SimpleBlockData, db::OnChainStorageRO};
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

#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingForChainHead,
    WaitingForSyncedBlock(SimpleBlockData),
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

    fn process_synced_block(self, block_hash: H256) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingForSyncedBlock(block) if block.hash == block_hash => {
                let validators = self
                    .ctx
                    .core
                    .db
                    .validators(self.ctx.core.timelines.era_from_ts(block.header.timestamp))
                    .ok_or(anyhow!("validators not found for block({block_hash})"))?;
                let producer = utils::block_producer_for(
                    &validators,
                    block.header.timestamp,
                    self.ctx.core.slot_duration.as_secs(),
                );
                let my_address = self.ctx.core.pub_key.to_address();

                if my_address == producer {
                    tracing::info!("👷 Start to work as a producer for block: {}", block.hash);

                    Producer::create(self.ctx, block.clone(), validators)
                } else {
                    // TODO #4636: add test (in ethexe-service) for case where is not validator for current block
                    let is_validator_for_current_block = validators.contains(&my_address);

                    if is_validator_for_current_block {
                        tracing::info!(
                            block = %block.hash,
                            producer = %producer,
                            "👷 Start to work as a subordinate for block, I am validator",
                        );
                    } else {
                        tracing::info!(
                            block = %block.hash,
                            producer = %producer,
                            "👷 Start to work as a subordinate for block, I am not a validator",
                        );
                    }

                    Subordinate::create(
                        self.ctx,
                        block.clone(),
                        producer,
                        is_validator_for_current_block,
                    )
                }
            }
            _ => DefaultProcessing::synced_block(self, block_hash),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConsensusEvent, validator::mock::*};
    use ethexe_common::{ValidatorsVec, db::*, mock::*};
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
        let (ctx, keys, _mock_eth) = mock_validator_context();
        let validators: ValidatorsVec = nonempty![
            ctx.core.pub_key.to_address(),
            keys[0].to_address(),
            keys[1].to_address(),
        ]
        .into();

        let block = BlockChain::mock(2).setup(&ctx.core.db).blocks[2].to_simple();
        ctx.core.db.set_validators(
            ctx.core.timelines.era_from_ts(block.header.timestamp),
            validators,
        );

        let initial = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        let producer = initial
            .process_synced_block(block.hash)
            .unwrap()
            .wait_for_state(|state| state.is_producer())
            .await
            .unwrap();
        assert!(producer.is_producer());
    }

    #[tokio::test]
    async fn switch_to_subordinate() {
        let (ctx, keys, _mock_eth) = mock_validator_context();

        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();

        let validators: ValidatorsVec = nonempty![
            ctx.core.pub_key.to_address(),
            keys[1].to_address(),
            keys[2].to_address(),
        ]
        .into();

        ctx.core.db.set_block_header(block.hash, block.header);
        ctx.core.db.set_validators(
            ctx.core.timelines.era_from_ts(block.header.timestamp),
            validators,
        );

        let initial = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        let state = initial.process_synced_block(block.hash).unwrap();
        let state = state
            .wait_for_state(|state| state.is_subordinate())
            .await
            .unwrap();
        assert!(state.is_subordinate());
    }

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

    #[test]
    fn producer_for_calculates_correct_producer() {
        let (_ctx, keys, _) = mock_validator_context();
        let validators = keys
            .iter()
            .map(|k| k.to_address())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let timestamp = 10;

        let producer = utils::block_producer_for(&validators, timestamp, 1);
        assert_eq!(producer, validators[10 % validators.len()]);
    }
}
