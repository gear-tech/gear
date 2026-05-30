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
            CoordinatorBoot::start(self.ctx, block, validators)
        } else {
            Participant::create(self.ctx, block, coordinator_addr)
        }
    }
}
