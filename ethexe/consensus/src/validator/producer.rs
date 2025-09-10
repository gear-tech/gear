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
    StateHandler, ValidatorContext, ValidatorState, coordinator::Coordinator, initial::Initial,
};
use crate::{
    ConsensusEvent, utils,
    validator::{CHAIN_DEEPNESS_THRESHOLD, MAX_CHAIN_DEEPNESS},
};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, ProducerBlock, SimpleBlockData,
    db::{BlockMetaStorageRead, NextEraValidators, OnChainStorageRead},
    ecdsa::PublicKey,
    end_of_era_timestamp, era_from_ts,
    gear::{
        AggregatedPublicKey, BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment,
        ValidatorsCommitment,
    },
};
use ethexe_service_utils::Timer;
use futures::FutureExt;
use gprimitives::{H256, U256};
use nonempty::NonEmpty;
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{self, IdentifierList},
};
use std::task::Context;

/// [`Producer`] is the state of the validator, which creates a new block
/// and publish it to the network. It waits for the block to be computed
/// and then switches to [`Coordinator`] state.
#[derive(Debug, Display)]
#[display("PRODUCER in {:?}", self.state)]
pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: NonEmpty<Address>,
    state: State,
}

#[derive(Debug)]
enum State {
    CollectCodes {
        #[debug(skip)]
        timer: Timer,
    },
    WaitingBlockComputed,
}

impl StateHandler for Producer {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_computed_block(mut self, computed_block: H256) -> Result<ValidatorState> {
        if !matches!(&self.state, State::WaitingBlockComputed if self.block.hash == computed_block)
        {
            self.warning(format!("unexpected computed block {computed_block}"));

            return Ok(self.into());
        }

        let batch = match Self::aggregate_batch_commitment(&self.ctx, &self.block)? {
            Some(batch) => batch,
            None => return Initial::create(self.ctx),
        };

        Coordinator::create(self.ctx, self.validators, batch)
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        match &mut self.state {
            State::CollectCodes { timer } => {
                if timer.poll_unpin(cx).is_ready() {
                    self.create_producer_block()?
                }
            }
            State::WaitingBlockComputed => {}
        }

        Ok(self.into())
    }
}

impl Producer {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: NonEmpty<Address>,
    ) -> Result<ValidatorState> {
        assert!(
            validators.contains(&ctx.pub_key.to_address()),
            "Producer is not in the list of validators"
        );

        let mut timer = Timer::new("collect codes", ctx.slot_duration / 6);
        timer.start(());

        ctx.pending_events.clear();

        Ok(Self {
            ctx,
            block,
            validators,
            state: State::CollectCodes { timer },
        }
        .into())
    }

    fn aggregate_batch_commitment(
        ctx: &ValidatorContext,
        block: &SimpleBlockData,
    ) -> Result<Option<BatchCommitment>> {
        let chain_commitment = Self::aggregate_chain_commitment(ctx, block.hash)?;
        let code_commitments = Self::aggregate_code_commitments(ctx, block.hash)?;
        let validators_commitment = Self::aggregate_validators_commitment(ctx, block.hash)?;
        let rewards_commitment = Self::aggregate_rewards_commitment(ctx, block.hash)?;

        if chain_commitment.is_none()
            && code_commitments.is_empty()
            && validators_commitment.is_none()
            && rewards_commitment.is_none()
        {
            log::debug!(
                "No commitments for block {} - skip batch commitment",
                block.hash
            );
            return Ok(None);
        }

        assert!(
            validators_commitment.is_none(),
            "TODO #4741: validators commitment is not supported yet"
        );
        assert!(
            rewards_commitment.is_none(),
            "TODO #4742: rewards commitment is not supported yet"
        );

        utils::create_batch_commitment(&ctx.db, block, chain_commitment, code_commitments)
    }

    fn aggregate_chain_commitment(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Option<ChainCommitment>> {
        let Some((commitment, deepness)) = utils::aggregate_chain_commitment(
            &ctx.db,
            block_hash,
            false,
            Some(MAX_CHAIN_DEEPNESS),
        )?
        else {
            return Ok(None);
        };

        if commitment.transitions.is_empty() && deepness <= CHAIN_DEEPNESS_THRESHOLD {
            // No transitions and chain is not deep enough, skip chain commitment
            Ok(None)
        } else {
            Ok(Some(commitment))
        }
    }

    fn aggregate_code_commitments(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>> {
        let queue = ctx
            .db
            .block_codes_queue(block_hash)
            .ok_or_else(|| anyhow!("Computed block {block_hash} codes queue is not in storage"))?;

        utils::aggregate_code_commitments(&ctx.db, queue, false)
    }

    // TODO #4741
    fn aggregate_validators_commitment(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Option<ValidatorsCommitment>> {
        let block_header = ctx.db.block_header(block_hash).ok_or(anyhow!(
            "block header not found in `aggregate validators commitment`"
        ))?;
        let block_era = era_from_ts(
            block_header.timestamp,
            ctx.timelines.genesis_ts,
            ctx.timelines.era,
        );
        let election_ts =
            end_of_era_timestamp(block_era, ctx.timelines.genesis_ts, ctx.timelines.era)
                - ctx.timelines.election;

        // No need to create validators commitment before the election timestamp
        if block_header.timestamp < election_ts {
            return Ok(None);
        }

        let validators_info = ctx.db.validators_info(block_hash).ok_or(anyhow!(
            "Validators info must be in storage for block {block_hash}"
        ))?;

        let next_validators = match validators_info.next {
            NextEraValidators::Unknown => {
                log::warn!("Validators are not elected in Observer, but should be");
                return Ok(None);
            }
            NextEraValidators::Elected(validators) => validators,
            NextEraValidators::Committed(_v) => {
                // No need to continue because of validators are already committed
                return Ok(None);
            }
        };

        let validators_identifiers = next_validators
            .iter()
            .map(|validator| Identifier::deserialize(&validator.0).unwrap())
            .collect::<Vec<_>>();
        let identifiers = IdentifierList::Custom(&validators_identifiers);

        let (mut secret_shares, public_key_package) =
            keys::generate_with_dealer(1, 1, identifiers, rand::thread_rng()).unwrap();

        let verifiable_secret_sharing_commitment = secret_shares
            .pop_first()
            .map(|(_key, value)| value.commitment().clone())
            .expect("Expect at least one identifier");

        let public_key_compressed: [u8; 33] = public_key_package
            .verifying_key()
            .serialize()?
            .try_into()
            .unwrap();
        let public_key_uncompressed = PublicKey(public_key_compressed).to_uncompressed();
        let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

        let aggregated_public_key = AggregatedPublicKey {
            x: U256::from_big_endian(public_key_x_bytes),
            y: U256::from_big_endian(public_key_y_bytes),
        };

        Ok(Some(ValidatorsCommitment {
            aggregated_public_key,
            verifiable_secret_sharing_commitment,
            validators: next_validators.into_iter().map(Into::into).collect(),
            // For next era from current block
            era_index: block_era + 1,
        }))
    }

    // TODO #4742
    fn aggregate_rewards_commitment(
        _ctx: &ValidatorContext,
        _block_hash: H256,
    ) -> Result<Option<RewardsCommitment>> {
        Ok(None)
    }

    fn create_producer_block(&mut self) -> Result<()> {
        let pb = ProducerBlock {
            block_hash: self.block.hash,
            // TODO #4638: set gas allowance here
            gas_allowance: None,
            // TODO #4639: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_pb = self.ctx.signer.signed_data(self.ctx.pub_key, pb.clone())?;

        self.state = State::WaitingBlockComputed;
        self.output(ConsensusEvent::PublishProducerBlock(signed_pb));
        self.output(ConsensusEvent::ComputeProducerBlock(pb));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SignedValidationRequest, mock::*, validator::mock::*};
    use ethexe_common::{
        Digest, ToDigest,
        db::{BlockMetaStorageWrite, OnChainStorageWrite, ValidatorsInfo},
    };
    use nonempty::{NonEmpty, nonempty};

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random());

        ctx.pending(SignedValidationRequest::mock((
            ctx.signer.clone(),
            keys[0],
            (),
        )));

        let producer = Producer::create(ctx, block, validators.clone()).unwrap();

        let ctx = producer.context();
        assert_eq!(
            ctx.pending_events.len(),
            0,
            "Producer must ignore external events"
        );
    }

    #[tokio::test]
    async fn simple() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.db, H256::random());
        ctx.db.set_validators_info(
            block.hash,
            ValidatorsInfo {
                current: nonempty![Address::default()],
                next: NextEraValidators::Elected(nonempty![Address::default()]),
            },
        );

        let producer = create_producer_skip_timer(ctx, block.clone(), validators)
            .await
            .unwrap()
            .0;

        // No commitments - no batch and goes to initial state
        let initial = producer.process_computed_block(block.hash).unwrap();
        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 0);
        with_batch(|batch| assert!(batch.is_none()));
    }

    #[tokio::test]
    async fn complex() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let batch = prepared_mock_batch_commitment(&ctx.db);
        let block = simple_block_data(&ctx.db, batch.block_hash);
        ctx.db.set_validators_info(
            block.hash,
            ValidatorsInfo {
                current: validators.clone(),
                next: NextEraValidators::Elected(validators.clone()),
            },
        );

        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap();
        assert!(submitter.is_submitter());
        assert_eq!(submitter.context().output.len(), 0);

        let initial = submitter.wait_for_event().await.unwrap().0;
        assert!(initial.is_initial());

        // Check that we have a batch with commitments after submitting
        let mut ctx = initial.into_context();
        with_batch(|multisigned_batch| {
            let (committed_batch, signatures) = multisigned_batch
                .cloned()
                .expect("Expected that batch is committed")
                .into_parts();

            assert_eq!(committed_batch, batch);
            assert_eq!(signatures.len(), 1);

            let (address, signature) = signatures.into_iter().next().unwrap();
            assert_eq!(
                signature
                    .validate(ctx.router_address, batch.to_digest())
                    .unwrap()
                    .to_address(),
                address
            );
        });

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        ctx.signatures_threshold = 2;
        let (coordinator, request) = create_producer_skip_timer(ctx, block.clone(), validators)
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            request,
            ConsensusEvent::PublishValidationRequest(_)
        ));
    }

    #[tokio::test]
    async fn code_commitments_only() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.db, H256::random());
        ctx.db.set_validators_info(
            block.hash,
            ValidatorsInfo {
                current: validators.clone(),
                next: NextEraValidators::Elected(validators.clone()),
            },
        );

        let code1 = CodeCommitment::mock(()).prepare(&ctx.db, ());
        let code2 = CodeCommitment::mock(()).prepare(&ctx.db, ());
        ctx.db
            .set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());
        ctx.db.mutate_block_meta(block.hash, |meta| {
            meta.last_committed_batch = Some(Digest::random());
            meta.last_committed_head = Some(H256::random());
        });

        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap();

        let initial = submitter.wait_for_event().await.unwrap().0;
        assert!(initial.is_initial());
        with_batch(|batch| {
            let batch = batch.expect("Expected that batch is committed");
            assert_eq!(batch.signatures().len(), 1);
            assert!(batch.batch().chain_commitment.is_none());
            assert_eq!(batch.batch().code_commitments.len(), 2);
        });
    }

    async fn create_producer_skip_timer(
        ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: NonEmpty<Address>,
    ) -> Result<(ValidatorState, ConsensusEvent, ConsensusEvent)> {
        let producer = Producer::create(ctx, block.clone(), validators)?;
        assert!(producer.is_producer());

        let (producer, publish_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(
            publish_event,
            ConsensusEvent::PublishProducerBlock(_)
        ));

        let (producer, compute_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(
            compute_event,
            ConsensusEvent::ComputeProducerBlock(_)
        ));

        Ok((producer, publish_event, compute_event))
    }
}
