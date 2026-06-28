// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-malachite-core
//!
//! Application-agnostic Malachite BFT consensus service used by `ethexe-malachite`.
//! Wraps the upstream `malachitebft-app-channel` engine, owns the persistent
//! BFT-side state, and exposes a minimal trait-based API so any application
//! can plug in without touching BFT plumbing. Network I/O is supplied by a
//! custom network adapter.
//!
//! `ethexe-malachite` is the only direct consumer: it supplies the [`Externalities`]
//! implementation and wraps [`MalachiteCore`] in its own facade exposed to the rest
//! of the ethexe stack. Block delivery happens exclusively through async
//! [`Externalities`] callbacks.
//!
//! - Block payload — a size-capped opaque byte string
//!   ([`BlockPayload`] — `LimitedVec<u8, `[`MAX_BLOCK_PAYLOAD_BYTES`]`>`) the application
//!   produces and consumes. The service wraps it into [`Block`] (adds
//!   `parent_hash`, `height`, `reserved`) and computes the canonical
//!   [`H256`] block hash via Blake2b-256; schema interpretation lives
//!   entirely in the application.
//! - [`Externalities`] — async callbacks the service invokes to
//!   process proposals, mark them finalized, build new ones (when
//!   proposer), and validate incoming proposals. These callbacks are
//!   the application's only signal that a block exists — the service
//!   no longer exposes a separate event stream.
//!
//! ## Usage
//!
//! Construct the service with [`MalachiteCore::new`](MalachiteCore), then poll it
//! as a `Stream`:
//!
//! ```rust,no_run
//! # use ethexe_malachite_core::{MalachiteCoreConfig, MalachiteCore, Externalities, NetworkMsg, NetworkRef};
//! # use std::sync::Arc;
//! # use tokio::sync::mpsc;
//! async fn start<EXT: Externalities>(
//!     config: MalachiteCoreConfig,
//!     network_ref: NetworkRef<MalachiteCtx>,
//!     tx_network: mpsc::Sender<NetworkMsg<MalachiteCtx>>,
//!     ext: Arc<EXT>,
//! ) -> MalachiteCore<EXT> {
//!     MalachiteCore::<EXT>::new(config, network_ref, tx_network, ext)
//!         .await
//!         .expect("service starts")
//! }
//! ```
//!
//! ## Public API
//!
//! - [`Externalities`] — Async application callbacks: `process_mb_proposal`, `process_mb_finalized`, `build_block_above`,
//!   `validate_block_above`.
//! - [`MalachiteCore`] — Running service; owns the engine, app task, WAL, and store. Network I/O is supplied by a custom network adapter.
//! - [`MService`] — Supertrait bound implemented by [`MalachiteCore`].
//! - [`Block`] — Service-level block envelope: `{ parent_hash: H256, height: u64, payload: BlockPayload, reserved: [u8; 64] }`.
//! - [`ValidatorEntry`] — Validator set member: `public_key` + `voting_power`, used in [`MalachiteCoreConfig`] and
//!   `update_validators`.
//! - [`MalachiteCoreConfig`] — Node configuration: validator secret, validator set, `persistent_peers`, propose timeout,
//!   [`NodeRole`], and `base` project directory.
//! - [`CommitCertificate`] — Finalization certificate delivered with `process_mb_finalized`.
//!
//! ## Caller invariants
//!
//! - `base` must be a persistent path: the service writes `<base>/malachite/` (store and
//!   WAL) on first run and resumes from it on restart; a transient path loses BFT state.
//! - Every persistent-peer multiaddr must include a `/p2p/<peer-id>` suffix (see
//!   the shared network peer id).
//! - Returning `Err` from a proposal or finalization callback is fatal: the error
//!   surfaces on the [`MalachiteCore`] stream and the consensus loop aborts rather
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
    codec::ScaleCodec,
    config::{MalachiteCoreConfig, Multiaddr, NodeRole, ValidatorEntry},
    context::MalachiteCtx,
    externalities::Externalities,
    service::{MService, MalachiteCore, MalachiteNetworkParts},
    signing::{
        MalachiteSigner, PrivateKey, PublicKey, Signature, derive_libp2p_secret,
        libp2p_keypair_from, libp2p_peer_id, private_key_from_bytes, private_key_from_gsigner,
        public_key_from_gsigner,
    },
    types::{Address, Block, BlockPayload, CommitCertificate, H256, MAX_BLOCK_PAYLOAD_BYTES},
};
pub use malachitebft_app_channel::NetworkMsg;
pub use malachitebft_engine::network::{
    NetworkEvent, NetworkMsg as EngineNetworkMsg, NetworkRef, Subscriber,
};

/// Re-exported libp2p PeerId — used by integration tests / operators
/// to materialize `/p2p/<peer-id>` multiaddr suffixes.
pub use libp2p_identity::PeerId;
