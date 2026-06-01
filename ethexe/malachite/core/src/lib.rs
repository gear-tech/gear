// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-malachite-core
//!
//! Application-agnostic Malachite BFT consensus service used by `ethexe-malachite`.
//! Wraps the upstream `malachitebft-app-channel` engine, owns the libp2p swarm and the
//! persistent BFT-side state, and exposes a minimal trait-based API so any application
//! can plug in without touching BFT plumbing.
//!
//! `ethexe-malachite` is the only direct consumer: it supplies the [`Externalities`]
//! implementation and wraps [`MalachiteService`] in its own facade exposed to the rest
//! of the ethexe stack. Block delivery happens exclusively through async
//! [`Externalities`] callbacks.
//!
//! ## Usage
//!
//! Construct the service with [`MalachiteService::new`](MalachiteService), then poll it
//! as a `Stream`:
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
//! ## Public API
//!
//! | Item | Description |
//! |------|-------------|
//! | [`Externalities`] | Async application callbacks: `process_mb_proposal`, `process_mb_finalized`, `build_block_above`, `validate_block_above`. |
//! | [`BlockPayload`] | Marker trait for the payload: `Clone + Encode + Decode + Send + Sync + 'static` (blanket impl). |
//! | [`MalachiteService`] | Running service; owns the swarm and store. Implements `Stream<Item = anyhow::Error>` and [`MService`]. `update_validators` rotates the active validator set, taking effect at the next height boundary. |
//! | [`MService`] | Supertrait bound implemented by [`MalachiteService`]. |
//! | [`Block`] | Service-level block envelope: `{ parent_hash: H256, height: u64, payload: P, reserved: [u8; 64] }`. |
//! | [`ValidatorEntry`] | Validator set member: `public_key` + `voting_power`, used in [`MalachiteConfig`] and `update_validators`. |
//! | [`MalachiteConfig`] | Node configuration: validator secret, validator set, `persistent_peers`, propose timeout, [`NodeRole`], `listen_addr`, and `base` project directory. |
//! | [`CommitCertificate`] | Finalization certificate delivered with `process_mb_finalized`. |
//! | [`libp2p_peer_id`] | Derive a [`PeerId`] from a validator secret offline, to build the `/p2p/<peer-id>` suffix of each persistent-peer multiaddr. |
//!
//! ## Caller invariants
//!
//! - `listen_addr` must be set explicitly; [`MalachiteConfig::DEFAULT_LISTEN_ADDR`] is
//!   available but is not applied by any struct-level default.
//! - `base` must be a persistent path: the service writes `<base>/malachite/` (store and
//!   WAL) on first run and resumes from it on restart; a transient path loses BFT state.
//! - Every persistent-peer multiaddr must include a `/p2p/<peer-id>` suffix (see
//!   [`libp2p_peer_id`]).
//! - Returning `Err` from a proposal or finalization callback is fatal: the error
//!   surfaces on the [`MalachiteService`] stream and the consensus loop aborts rather
//!   than skipping the callback.
//!
//! The service guarantees a strictly linearised block stream: each block is delivered
//! only after every ancestor was processed and finalized in order.

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
