// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application callbacks the service makes to the outside world.

use crate::{
    config::ValidatorPublicKey,
    types::{Block, BlockPayload, CommitCertificate, H256},
};
use anyhow::Result;
use async_trait::async_trait;
use ethexe_common::Acceptance;

/// Application-side callbacks the consensus service requires.
/// The application contributes only the opaque [`BlockPayload`];
/// everything BFT-related stays inside the service.
///
/// Guaranteed happens-before ordering (`H256::zero()` parent = genesis):
///
/// 1. [`Self::process_mb_proposal`] fires once per `mb_hash`, only after
///    every ancestor's `process_mb_proposal` returned `Ok`. Sibling
///    proposals at the same height are possible.
/// 2. [`Self::process_mb_finalized`] fires only after the same hash's
///    `process_mb_proposal` and every ancestor's finalization.
/// 3. [`Self::build_block_above`] / [`Self::validate_block_above`] fire
///    only after the parent has been finalized.
#[async_trait]
pub trait Externalities: Send + Sync + 'static {
    /// Persist `block` indexed by `mb_hash`; called exactly once per hash
    /// at proposal-assembly time.
    async fn process_mb_proposal(&self, mb_hash: H256, block: Block) -> Result<()>;

    /// Mark `mb_hash` as finalized; `cert` is the BFT commit certificate
    /// for its height.
    async fn process_mb_finalized(&self, mb_hash: H256, cert: CommitCertificate) -> Result<()>;

    /// Build a fresh block payload on top of `parent_mb_hash`; called only
    /// when this node is the elected proposer.
    ///
    /// The future may wait arbitrarily long — the service races it against
    /// [`crate::MalachiteCoreConfig::propose_timeout`] and cancels it on
    /// timeout, so implementations must be cancellation-safe.
    async fn build_block_above(&self, parent_mb_hash: H256) -> Result<BlockPayload>;

    /// Application-side validation of an incoming proposal's **payload
    /// only** — parent linkage and height are checked by the consensus
    /// layer before this hook fires.
    ///
    /// Returns `Accepted` to vote for the proposal, `Rejected(reason)` to
    /// vote nil, `Err(_)` for an unexpected internal failure. Not called
    /// on the sync path (sync values carry a quorum certificate).
    async fn validate_block_above(
        &self,
        parent_mb_hash: H256,
        payload: &BlockPayload,
    ) -> Result<Acceptance<(), String>>;

    /// Resolve the on-chain validator set that governs the **child** MB of
    /// `parent_mb_hash` (zero = the genesis MB's parent). The governing era is
    /// the one the parent's last `AdvanceTillEthereumBlock` landed in, so once
    /// a producer advances into the next era every descendant MB is built and
    /// validated against that era's set. Used by the consensus layer to verify
    /// each height's commit certificate against the correct historical set
    /// during sync, not just the live shared set.
    fn validators_for_child_of(&self, parent_mb_hash: H256) -> Result<Vec<ValidatorPublicKey>>;
}
