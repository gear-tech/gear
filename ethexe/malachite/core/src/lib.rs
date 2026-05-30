// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-malachite-core
//!
//! Application-agnostic Malachite BFT consensus service used by `ethexe-malachite`.
//! Wraps the upstream `malachitebft-app-channel` engine, owns the libp2p swarm and
//! the persistent BFT-side state (RocksDB store), and exposes a minimal trait-based
//! API so any application can plug in without touching BFT plumbing.
//!
//! ## Responsibilities
//!
//! - Wraps an application payload `P` into a service-level [`Block`] envelope
//!   (`parent_hash`, `height`, `reserved`) and computes the canonical [`H256`] block
//!   hash via Blake2b-256 over a SCALE-encoded tuple.
//! - Drives the BFT engine and delivers a *linearised block stream* to the application
//!   exclusively through async [`Externalities`] callbacks — propose, finalize, build,
//!   and validate — with strict happens-before ordering guarantees.
//! - Manages secp256k1 keys and 20-byte [`Address`] identities (via `gsigner`), and
//!   derives libp2p peer identities from the same key material.
//!
//! ## Role in the Stack
//!
//! ```text
//! ethexe-malachite  (EthexeExternalities, mempool, MalachiteService facade)
//!        │
//!        └─ ethexe-malachite-core   ← this crate
//!               │  owns: libp2p swarm, RocksDB BFT store
//!               │  drives: malachitebft-app-channel engine
//!               └─ application (implements Externalities<P>)
//! ```
//!
//! `ethexe-malachite` is the only direct consumer. It supplies `EthexeExternalities`
//! and re-exports `MalachiteService` to the rest of the ethexe stack.
//!
//! ## Entry Points / Public API
//!
//! Construct the service with [`MalachiteService::new`](MalachiteService):
//!
//! ```rust,no_run
//! # use ethexe_malachite_core::{MalachiteConfig, MalachiteService, Externalities, BlockPayload};
//! # use std::sync::Arc;
//! async fn start<P: BlockPayload, EXT: Externalities<P>>(
//!     config: MalachiteConfig,
//!     ext: Arc<EXT>,
//! ) -> MalachiteService<P, EXT> {
//!     MalachiteService::<P, EXT>::new(config, ext)
//!         .await
//!         .expect("service starts")
//! }
//! ```
//!
//! Poll the returned service as a `Stream`; it yields only fatal `anyhow::Error`s.
//! All block delivery happens through the [`Externalities`] callbacks — there is no
//! separate success-event stream.
//!
//! ## Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`Externalities`] | Async application callbacks: `process_mb_proposal`, `process_mb_finalized`, `build_block_above`, `validate_block_above`. |
//! | [`BlockPayload`] | Marker trait for the payload: `Clone + Encode + Decode + Send + Sync + 'static` (blanket impl). |
//! | [`MalachiteService`] | Running service; owns the swarm and store. Implements `Stream<Item = anyhow::Error>` and [`MService`]. Call `update_validators(&self, validators: Vec<ValidatorEntry>) -> Result<()>` to rotate the active validator set; the swap takes effect at the next height boundary (the running height completes with the previous set). |
//! | [`MService`] | Dyn-compatible facade trait for [`MalachiteService`]. |
//! | [`Block`] | Service-level block envelope: `{ parent_hash: H256, height: u64, payload: P, reserved: [u8; 64] }`. |
//! | [`ValidatorEntry`] | Struct populated in `MalachiteConfig::validators` and passed to `update_validators`: `public_key: gsigner::schemes::secp256k1::PublicKey` + `voting_power: u64`. |
//! | [`MalachiteConfig`] | Node configuration: validator secret, validator set, peer addresses, propose timeout, [`NodeRole`], `listen_addr` (local TCP address; the constant `MalachiteConfig::DEFAULT_LISTEN_ADDR` is `0.0.0.0:20334`, but callers must set `listen_addr` explicitly — no struct-level default exists), and `base` (project directory; the service writes `<base>/malachite/` on first run — containing `store.db` and `consensus.wal` — and resumes from it on restart; using a transient path silently loses BFT state). |
//! | [`CommitCertificate`] | Finalization certificate delivered with `process_mb_finalized`. |
//!
//! ## Key Functions
//!
//! | Function | Description |
//! |----------|-------------|
//! | `libp2p_peer_id(validator_secret: &[u8; 32]) -> PeerId` | Derive the libp2p [`PeerId`] from a validator secret key offline, without starting the engine. Needed to construct the `/p2p/<peer-id>` suffix of each `MalachiteConfig::persistent_peers` entry (the config doc requires every persistent-peer multiaddr to include this suffix so the swarm knows who to expect on the other side). |
//!
//! ## Ordering Invariants
//!
//! The service guarantees a strictly linearised block stream to the application:
//!
//! - `process_mb_proposal(block_hash, block)` — called only after every ancestor has
//!   already returned successfully from a prior `process_mb_proposal` call. Siblings
//!   at the same height (one per fork-causing round) are each delivered exactly once.
//! - `process_mb_finalized(block_hash, cert)` — called only after `process_mb_proposal`
//!   for `block_hash` succeeded and every ancestor was finalized.
//! - `build_block_above` / `validate_block_above` — called only after the parent block
//!   is finalized, or when `parent_hash == H256::zero()` for the genesis block.
//!
//! Returning `Err` from a proposal or finalization callback is fatal: the block stays
//! unsaved in the store, the error surfaces on the `MalachiteService` stream, and the
//! consensus loop aborts rather than silently skipping the callback.

mod config;
mod externalities;
mod service;
mod types;

// Implementation modules.
mod app;
mod codec;
mod context;
mod signing;
mod state;
mod store;
mod streaming;

pub use crate::{
    config::{MalachiteConfig, Multiaddr, NodeRole, ValidatorEntry},
    externalities::{BlockPayload, Externalities},
    service::{MService, MalachiteService},
    signing::{
        MalachiteSigner, PrivateKey, PublicKey, Signature, derive_libp2p_secret,
        libp2p_keypair_from, libp2p_peer_id, private_key_from_bytes, private_key_from_gsigner,
        public_key_from_gsigner,
    },
    types::{Address, Block, CommitCertificate, H256},
};

/// Re-exported libp2p PeerId — used by integration tests / operators
/// to materialize `/p2p/<peer-id>` multiaddr suffixes.
pub use libp2p_identity::PeerId;
