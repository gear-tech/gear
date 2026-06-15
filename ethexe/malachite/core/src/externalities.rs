// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application callbacks the service makes to the outside world.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

use crate::types::{Address, Block, BlockPayload, CommitCertificate, H256};

/// Application-side callbacks the consensus service requires.
///
/// The service is application-agnostic: it owns the BFT engine, the
/// libp2p swarm, and the persistent BFT state. The opaque, size-capped
/// payload byte string ([`BlockPayload`]) is the only
/// shape the application contributes to a [`Block`] — encoding and
/// decoding of any application-level schema lives behind this trait.
///
/// The service guarantees a strict happens-before ordering for the
/// callbacks below — the application never has to maintain its own
/// synchronization barrier:
///
/// 1. [`Self::process_mb_proposal`] for `mb_hash` is called as soon
///    as a proposal carrying that hash has been assembled and
///    validated locally (regardless of whether this node is the
///    proposer, a participant that received the proposal over the
///    network, or a full node that received a synced decided value),
///    but **only after** every ancestor of `mb_hash` has already
///    returned successfully from a previous `process_mb_proposal`
///    call. Sibling proposals at the same height are possible (one
///    per fork-causing round) and each is delivered exactly once
///    per `mb_hash`.
/// 2. [`Self::process_mb_finalized`] for `mb_hash` is called **only
///    after** `process_mb_proposal` for that same `mb_hash` returned
///    successfully **and** every ancestor has already been finalized
///    via previous `process_mb_finalized` calls.
/// 3. [`Self::build_block_above`] / [`Self::validate_block_above`]
///    are called only after the parent has been finalized (or
///    `parent_hash == H256::zero()` when building / validating the
///    genesis block).
///
/// All methods are async; the service `await`s them inline.
#[async_trait]
pub trait Externalities: Send + Sync + 'static {
    /// Persist `block` indexed by `mb_hash`. Called exactly once
    /// per `mb_hash` over the lifetime of an application instance,
    /// at proposal-assembly time, after every ancestor's
    /// `process_mb_proposal` has already returned `Ok`.
    async fn process_mb_proposal(&self, mb_hash: H256, block: Block) -> Result<()>;

    /// Mark `mb_hash` as finalized and durable.
    ///
    /// `cert` is the BFT commit certificate for the height of
    /// `mb_hash`. The application typically forwards `cert` to
    /// downstream layers (on-chain commits, light clients, etc.).
    async fn process_mb_finalized(
        &self,
        mb_hash: H256,
        cert: CommitCertificate,
        extensions: Vec<(Address, Bytes)>,
    ) -> Result<()>;

    /// Build an optional opaque vote extension for the block this node is about
    /// to precommit.
    async fn extend_vote(&self, _mb_hash: H256, _block: Block) -> Result<Option<Bytes>> {
        Ok(None)
    }

    /// Application-side validation for an opaque vote extension.
    async fn verify_vote_extension(
        &self,
        _mb_hash: H256,
        _block: Block,
        _extension: Bytes,
    ) -> Result<bool> {
        Ok(false)
    }

    /// Build a fresh block payload whose parent has hash
    /// `parent_mb_hash`. Called only when this node has been elected
    /// proposer. The new block's height is derivable from `parent_mb_hash`
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
    async fn build_block_above(&self, parent_mb_hash: H256) -> Result<BlockPayload>;

    /// Application-side validation of an incoming proposal's
    /// **payload only**.
    ///
    /// Parent linkage and height progression are validated inside
    /// the consensus layer before this hook fires; the caller still
    /// passes `parent_mb_hash` for context (e.g. to read ancestor state
    /// from an application-side store) but is not expected to
    /// re-check `block.parent_mb_hash`. `parent_mb_hash == H256::zero()`
    /// signals the genesis block.
    ///
    /// Typical responsibilities:
    /// - the payload bytes decode against the application's schema;
    /// - the decoded content is well-formed against the application's
    ///   protocol invariants (gas budget, single anchor advance,
    ///   operation shape, etc.).
    /// - Optionally a stronger proposer-authorization check on top
    ///   of malachite's validator set.
    ///
    /// Returns `Ok(true)` to vote for the proposal, `Ok(false)` to
    /// reject without crashing, `Err(_)` for an unexpected internal
    /// failure (surfaces as an error event on the service stream).
    ///
    /// Not called on the sync path — sync values come with a quorum
    /// commit certificate and are accepted on that basis alone.
    async fn validate_block_above(
        &self,
        parent_mb_hash: H256,
        payload: BlockPayload,
    ) -> Result<bool>;
}
