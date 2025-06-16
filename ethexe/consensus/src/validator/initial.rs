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
    producer::Producer, subordinate::Subordinate, DefaultProcessing, StateHandler,
    ValidatorContext, ValidatorState,
};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{Address, SimpleBlockData};
use ethexe_observer::BlockSyncedData;

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

    fn process_synced_block(self, data: BlockSyncedData) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingForSyncedBlock(block) if block.hash == data.block_hash => {
                let producer = self.producer_for(block.header.timestamp, &data.validators);
                let my_address = self.ctx.pub_key.to_address();

                if my_address == producer {
                    log::info!("👷 Start to work as a producer for block: {}", block.hash);

                    Producer::create(self.ctx, block.clone(), data.validators)
                } else {
                    // TODO #4636: add test (in ethexe-service) for case where is not validator for current block
                    let is_validator_for_current_block = data.validators.contains(&my_address);

                    log::info!(
                        "👷 Start to work as a subordinate for block: {}, producer is {producer}, \
                        I'm validator for current block: {is_validator_for_current_block}",
                        block.hash
                    );

                    Subordinate::create(
                        self.ctx,
                        block.clone(),
                        producer,
                        is_validator_for_current_block,
                    )
                }
            }
            _ => DefaultProcessing::synced_block(self, data),
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

    // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.
    pub fn create_with_chain_head(
        ctx: ValidatorContext,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        Ok(Self {
            ctx,
            state: State::WaitingForSyncedBlock(block),
        }
        .into())
    }

    fn producer_for(&self, timestamp: u64, validators: &[Address]) -> Address {
        let slot = timestamp / self.ctx.slot_duration.as_secs();
        let index = crate::block_producer_index(validators.len(), slot);
        validators
            .get(index)
            .cloned()
            .unwrap_or_else(|| unreachable!("index must be valid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*, ConsensusEvent};
    use gprimitives::H256;

    #[test]
    fn create_initial_success() {
        let (ctx, _) = mock_validator_context();
        let initial = Initial::create(ctx).unwrap();
        assert!(initial.is_initial());
    }

    #[test]
    fn create_with_chain_head_success() {
        let (ctx, _) = mock_validator_context();
        let block = mock_simple_block_data();
        let initial = Initial::create_with_chain_head(ctx, block.clone()).unwrap();
        assert!(initial.is_initial());
    }

    #[tokio::test]
    async fn switch_to_producer() {
        let (ctx, keys) = mock_validator_context();
        let validators = vec![
            ctx.pub_key.to_address(),
            keys[0].to_address(),
            keys[1].to_address(),
        ];

        let mut block = mock_simple_block_data();
        block.header.timestamp = 0;

        let data = BlockSyncedData {
            block_hash: block.hash,
            validators: validators.clone(),
        };

        let initial = Initial::create_with_chain_head(ctx, block).unwrap();
        let producer = initial.process_synced_block(data).unwrap();
        assert!(producer.is_producer());
    }

    #[test]
    fn switch_to_subordinate() {
        let (ctx, keys) = mock_validator_context();
        let validators = vec![
            ctx.pub_key.to_address(),
            keys[1].to_address(),
            keys[2].to_address(),
        ];

        let mut block = mock_simple_block_data();
        block.header.timestamp = 1;

        let data = BlockSyncedData {
            block_hash: block.hash,
            validators: validators.clone(),
        };

        let initial = Initial::create_with_chain_head(ctx, block).unwrap();
        let producer = initial.process_synced_block(data).unwrap();
        assert!(producer.is_subordinate());
    }

    #[test]
    fn process_synced_block_rejected() {
        let (ctx, _) = mock_validator_context();
        let block = mock_simple_block_data();
        let data = BlockSyncedData {
            block_hash: block.hash,
            validators: vec![],
        };

        let initial = Initial::create(ctx)
            .unwrap()
            .process_synced_block(data)
            .unwrap();
        assert!(initial.is_initial());
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));

        let data = BlockSyncedData {
            block_hash: H256::random(),
            validators: vec![],
        };

        let initial = initial
            .process_new_head(block)
            .unwrap()
            .process_synced_block(data)
            .unwrap();
        assert!(initial.is_initial());
        assert!(matches!(
            initial.context().output[1],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn producer_for_calculates_correct_producer() {
        let (ctx, keys) = mock_validator_context();
        let validators: Vec<_> = keys.iter().map(|k| k.to_address()).collect();
        let timestamp = 10;

        let producer = Initial {
            ctx,
            state: State::WaitingForChainHead,
        }
        .producer_for(timestamp, &validators);
        assert_eq!(producer, validators[10 % validators.len()]);
    }
}
