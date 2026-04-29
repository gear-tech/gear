// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

//! Application callbacks the service makes to the outside world.

use anyhow::Result;
use async_trait::async_trait;
use parity_scale_codec::{Decode, Encode};

use crate::types::{Block, CommitCertificate, H256};

/// Marker trait for application block payloads.
///
/// Any type that is `Clone + Encode + Decode + Send + Sync + 'static`
/// qualifies — the service never inspects the payload's contents,
/// it just SCALE-encodes it as part of [`Block`] for gossip / WAL /
/// RocksDB. The blanket impl below means application code never has
/// to write `impl BlockPayload for ... {}`.
pub trait BlockPayload: Clone + Encode + Decode + Send + Sync + 'static {}

impl<T> BlockPayload for T where T: Clone + Encode + Decode + Send + Sync + 'static {}

/// Application-side callbacks the consensus service requires.
///
/// The service guarantees a strict happens-before ordering for the
/// callbacks below — the application never has to maintain its own
/// synchronization barrier:
///
/// 1. [`Self::save_block`] for `block_hash` is called **only after**
///    every ancestor of `block_hash` has already returned successfully
///    from a previous `save_block` call.
/// 2. [`Self::mark_block_as_finalized`] for `block_hash` is called
///    **only after** `save_block` for that same `block_hash` returned
///    successfully **and** every ancestor has already been finalized
///    i.e. marked finalized via previous `mark_block_as_finalized` calls.
/// 3. [`Self::build_block_above`] / [`Self::validate_block_above`]
///    are called only after the parent has been finalized (or
///    `parent_hash == H256::zero()` when building / validating the
///    genesis block).
///
/// All methods are async; the service `await`s them inline.
/// Returning `Err` from `save_block` / `mark_block_as_finalized` is
/// treated as a fatal application error (the service propagates it
/// as a terminating error on its event stream).
#[async_trait]
pub trait Externalities<P: BlockPayload>: Send + Sync + 'static {
    /// Persist `block` indexed by `block_hash`. Called exactly once
    /// per `block_hash` over the lifetime of an application instance,
    /// and only after every ancestor has already been saved.
    async fn save_block(&self, block_hash: H256, block: Block<P>) -> Result<()>;

    /// Mark `block_hash` as finalized and durable.
    ///
    /// `cert` is the BFT commit certificate for the height of
    /// `block_hash`. The application typically forwards `cert` to
    /// downstream layers (on-chain commits, light clients, etc.).
    async fn mark_block_as_finalized(
        &self,
        block_hash: H256,
        cert: CommitCertificate,
    ) -> Result<()>;

    /// Build a fresh block payload whose parent has hash
    /// `parent_hash`. Called only when this node has been elected
    /// proposer. The new block's height is derivable from `parent_hash`
    /// (parent.height + 1, or 1 for genesis), so it isn't passed
    /// explicitly here.
    ///
    /// The future may take an arbitrarily long time — for example to
    /// wait on a mempool, an external block source, or a chain head
    /// — and the service races it against
    /// [`crate::MalachiteConfig::propose_timeout`]. On timeout the
    /// future is cancelled (dropped); implementations must be
    /// cancellation-safe.
    ///
    /// `parent_hash == H256::zero()` is passed when building the
    /// genesis block.
    async fn build_block_above(&self, parent_hash: H256) -> Result<P>;

    /// Application-side validation of an incoming proposal's
    /// **payload only**.
    ///
    /// Parent linkage and height progression are validated inside
    /// the consensus layer before this hook fires; the caller still
    /// passes `parent_hash` for context (e.g. to read ancestor state
    /// from an application-side store) but is not expected to
    /// re-check `block.parent_hash`. `parent_hash == H256::zero()`
    /// signals the genesis block.
    ///
    /// Typical responsibilities:
    /// - the payload is well-formed against the application's
    ///   protocol invariants (gas budget, single anchor advance,
    ///   transaction shape, etc.).
    /// - Optionally a stronger proposer-authorization check on top
    ///   of malachite's validator set.
    ///
    /// Returns `Ok(true)` to vote for the proposal, `Ok(false)` to
    /// reject without crashing, `Err(_)` for an unexpected internal
    /// failure (surfaces as an error event on the service stream).
    ///
    /// Not called on the sync path — sync values come with a quorum
    /// commit certificate and are accepted on that basis alone.
    async fn validate_block_above(&self, parent_hash: H256, payload: P) -> Result<bool>;
}
