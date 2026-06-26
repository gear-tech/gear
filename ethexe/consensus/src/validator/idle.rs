// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`Idle`] is the idle state of the MB-driven validator.
//!
//! It tracks three sub-states inline:
//!  1. waiting for a fresh chain head;
//!  2. waiting for that head to be synced;
//!  3. waiting for that head to be prepared (events processed).
//!
//! Once the block is prepared, the validator looks up which validator the
//! protocol elected as **coordinator** for this Ethereum block timestamp
//! and switches to either [`Coordinator`] or [`Participant`] accordingly.
//!
//! Coordinator election is independent of Malachite — it's a deterministic
//! function of `(timelines, validator set, block timestamp)`. See
//! [`ProtocolTimelines::block_coordinator_at`].

use super::{
    Participant, StateHandler, ValidatorContext, ValidatorState, coordinator::CoordinatorBoot,
};
use anyhow::{Context as _, Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    SimpleBlockData,
    db::{BlockMetaStorageRO, OnChainStorageRO},
};
use gprimitives::H256;

/// Idle state — waits for the next Ethereum chain head and then routes to
/// either [`Coordinator`] or [`Participant`] for that block.
#[derive(Debug, Display)]
#[display("IDLE in state {state:?}")]
pub struct Idle {
    ctx: ValidatorContext,
    state: SubState,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
enum SubState {
    /// Waiting for `receive_new_chain_head`.
    WaitingForChainHead,
    /// Got the head; waiting for it to be synced.
    WaitingForSynced { block: SimpleBlockData },
    /// Synced; waiting for it to be prepared (events processed).
    WaitingForPrepared { block: SimpleBlockData },
}

impl StateHandler for Idle {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        Self::create_with_chain_head(self.ctx, block)
    }

    fn process_synced_block(mut self, block: H256) -> Result<ValidatorState> {
        match &self.state {
            SubState::WaitingForSynced { block: pending } if pending.hash == block => {
                let pending = *pending;
                self.state = SubState::WaitingForPrepared { block: pending };
                self.maybe_advance_to_role()
            }
            _ => {
                tracing::trace!(
                    received = %block,
                    "synced block skipped - not waiting for this block",
                );
                Ok(self.into())
            }
        }
    }

    fn process_prepared_block(self, block: H256) -> Result<ValidatorState> {
        match &self.state {
            SubState::WaitingForPrepared { block: pending } if pending.hash == block => {
                self.maybe_advance_to_role()
            }
            _ => {
                tracing::trace!(
                    received = %block,
                    "prepared block skipped - not waiting for this block",
                );
                Ok(self.into())
            }
        }
    }
}

impl Idle {
    /// Enter idle state — equivalent to "no chain head observed yet".
    pub fn create(ctx: ValidatorContext) -> Result<ValidatorState> {
        Ok(Self {
            ctx,
            state: SubState::WaitingForChainHead,
        }
        .into())
    }

    /// Enter idle state already armed with a chain head — used both by the
    /// initial `receive_new_chain_head` and by every state that resets
    /// itself when a new head arrives mid-flight.
    pub fn create_with_chain_head(
        ctx: ValidatorContext,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        let s = Self {
            ctx,
            state: SubState::WaitingForSynced { block },
        };
        s.maybe_advance_to_role()
    }

    /// If the current sub-state matches what's already in the DB, fast-forward.
    fn maybe_advance_to_role(mut self) -> Result<ValidatorState> {
        // Auto-advance synced → prepared if DB already has the data.
        if let SubState::WaitingForSynced { block } = &self.state
            && self.ctx.core.db.block_synced(block.hash)
        {
            let block = *block;
            self.state = SubState::WaitingForPrepared { block };
        }

        let SubState::WaitingForPrepared { block } = self.state else {
            return Ok(self.into());
        };

        if !self.ctx.core.db.block_meta(block.hash).prepared {
            // Stay parked.
            return Ok(Self {
                ctx: self.ctx,
                state: SubState::WaitingForPrepared { block },
            }
            .into());
        }

        // Block is prepared — figure out who's coordinator and dispatch.
        let validators = {
            let timelines = self.ctx.core.timelines;
            let block_era = timelines
                .era_from_ts(block.header.timestamp)
                .context("failed to calculate era from block timestamp")?;
            self.ctx
                .core
                .db
                .validators(block_era)
                .ok_or_else(|| anyhow!("validators not found for era {block_era}"))?
        };

        let coordinator_addr = self
            .ctx
            .core
            .timelines
            .block_coordinator_at(&validators, block.header.timestamp)
            .ok_or_else(|| anyhow!("cannot determine coordinator for block {}", block.hash))?;

        if coordinator_addr == self.ctx.core.pub_key.to_address() {
            // The period is a coordinator-local cadence: only the elected
            // coordinator decides whether this block is a commitment block,
            // using its own `batch_commitment_period`. On a non-multiple block
            // it produces nothing and drops back to idle.
            let period = self.ctx.core.batch_commitment_period.get();
            if !block.header.height.is_multiple_of(period) {
                tracing::trace!(
                    block = %block.hash,
                    height = block.header.height,
                    period,
                    "coordinator skips this block: height not a multiple of batch_commitment_period",
                );
                return Idle::create(self.ctx);
            }

            CoordinatorBoot::start(self.ctx, block, validators)
        } else {
            // Participants always enter the role and validate whatever the
            // coordinator chooses to commit — the period is not consulted here,
            // so the knob stays purely coordinator-local.
            Participant::create(self.ctx, block, coordinator_addr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::{
        ValidatorMetrics,
        batch::{BatchCommitmentManager, BatchLimits},
        core::{BatchCommitter, MiddlewareWrapper, ValidatorCore},
    };
    use async_trait::async_trait;
    use ethexe_common::{
        Address,
        db::ConfigStorageRO,
        ecdsa::{ContractSignature, PublicKey},
        gear::BatchCommitment,
        mock::{BlockChain, Mock},
    };
    use ethexe_db::Database;
    use ethexe_ethereum::middleware::{ElectionProvider, MockElectionProvider};
    use gsigner::secp256k1::Signer;
    use std::{collections::VecDeque, num::NonZero, time::Duration};

    /// Committer that never touches the chain — the gate test only inspects
    /// the resulting state, no batch is ever submitted.
    struct NoopCommitter;

    #[async_trait]
    impl BatchCommitter for NoopCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(NoopCommitter)
        }

        async fn commit(
            self: Box<Self>,
            _batch: BatchCommitment,
            _signatures: Vec<ContractSignature>,
        ) -> Result<H256> {
            Ok(H256::zero())
        }
    }

    fn test_context(
        db: Database,
        signer: Signer,
        pub_key: PublicKey,
        batch_commitment_period: NonZero<u32>,
    ) -> ValidatorContext {
        let timelines = db.config().timelines;
        let middleware = MiddlewareWrapper::from_inner(
            Box::new(MockElectionProvider::new()) as Box<dyn ElectionProvider>
        );
        let batch_manager =
            BatchCommitmentManager::new(BatchLimits::default(), db.clone(), middleware);

        ValidatorContext {
            core: ValidatorCore {
                signatures_threshold: 1,
                router_address: Address([0; 20]),
                pub_key,
                timelines,
                signer,
                db,
                committer: Box::new(NoopCommitter),
                batch_manager,
                metrics: ValidatorMetrics::default(),
                commitment_delay_limit: ethexe_common::DEFAULT_COMMITMENT_DELAY_LIMIT,
                coordinator_aggregation_delay: Duration::ZERO,
                batch_commitment_period,
            },
            pending_events: VecDeque::new(),
            output: VecDeque::new(),
            tasks: Default::default(),
        }
    }

    /// The period is coordinator-local: when this node is the elected
    /// coordinator (single-validator set), `batch_commitment_period = 2` makes
    /// it build a batch only on even-height blocks and stay idle on odd ones.
    #[tokio::test]
    async fn batch_commitment_period_gates_coordinator_by_block_height() {
        let db = Database::memory();
        let signer = Signer::memory();
        let pub_key = signer.generate().unwrap();

        let mut chain = BlockChain::mock(10);
        chain.validators = vec![pub_key.to_address()].try_into().unwrap();
        let chain = chain.setup(&db);

        let period = NonZero::new(2).unwrap();

        // `blocks[i].height = genesis_height + i`; genesis_height is even,
        // so blocks[2] is even (a multiple of 2) and blocks[3] is odd.
        let even_block = chain.blocks[2].to_simple();
        let odd_block = chain.blocks[3].to_simple();
        assert!(even_block.header.height.is_multiple_of(period.get()));
        assert!(!odd_block.header.height.is_multiple_of(period.get()));

        // Odd height → coordinator skips → back to idle, waiting for next head.
        let ctx = test_context(db.clone(), signer.clone(), pub_key, period);
        let state = Idle::create(ctx)
            .unwrap()
            .process_new_head(odd_block)
            .unwrap();
        assert!(
            state.is_idle(),
            "odd-height block must be skipped by the coordinator, got {state}"
        );

        // Even height → coordinator boots and starts building a batch.
        let ctx = test_context(db, signer, pub_key, period);
        let state = Idle::create(ctx)
            .unwrap()
            .process_new_head(even_block)
            .unwrap();
        assert!(
            state.is_coordinator_boot(),
            "even-height block must start the coordinator, got {state}"
        );
    }

    /// A participant ignores its own period entirely: it enters the
    /// `Participant` role to validate the coordinator's batch even on a block
    /// whose height is not a multiple of the (large) local period.
    #[tokio::test]
    async fn participant_enters_regardless_of_batch_commitment_period() {
        let db = Database::memory();
        let signer = Signer::memory();
        let my_key = signer.generate().unwrap();
        let other_signer = Signer::memory();
        let other_key = other_signer.generate().unwrap();

        let validators_vec: ethexe_common::ValidatorsVec =
            vec![my_key.to_address(), other_key.to_address()]
                .try_into()
                .unwrap();
        let mut chain = BlockChain::mock(10);
        chain.validators = validators_vec.clone();
        let chain = chain.setup(&db);

        let timelines = db.config().timelines;

        // Find a prepared block whose elected coordinator is NOT this node —
        // there our node must always become a participant.
        let participant_block = chain
            .blocks
            .iter()
            .filter_map(|b| b.synced.as_ref().map(|_| b.to_simple()))
            .find(|b| {
                timelines.block_coordinator_at(&validators_vec, b.header.timestamp)
                    != Some(my_key.to_address())
            })
            .expect("expected a block coordinated by the other validator");

        // A period far larger than any height — proving the participant path
        // never consults it.
        let huge_period = NonZero::new(u32::MAX).unwrap();
        assert!(
            !participant_block
                .header
                .height
                .is_multiple_of(huge_period.get())
        );

        let ctx = test_context(db, signer, my_key, huge_period);
        let state = Idle::create(ctx)
            .unwrap()
            .process_new_head(participant_block)
            .unwrap();
        assert!(
            state.is_participant(),
            "participant must enter regardless of period, got {state}"
        );
    }
}
