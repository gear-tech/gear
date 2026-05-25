// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-malachite-core
//!
//! Application-agnostic Malachite BFT consensus service.
//!
//! Wraps the upstream `malachitebft-app-channel` engine, owns the
//! libp2p swarm and the persistent BFT-side state, and exposes a
//! minimal trait-based API so any application can plug in:
//!
//! - [`BlockPayload`] — marker trait the application's payload type
//!   must satisfy (`Clone + Encode + Decode + Send + Sync + 'static`,
//!   covered by a blanket impl). The service wraps the payload into
//!   [`Block`] itself (adds `parent_hash`, `height`, `reserved`) and
//!   computes the canonical [`H256`] block hash via Blake2b-256.
//! - [`Externalities`] — async callbacks the service invokes to
//!   process proposals, mark them finalized, build new ones (when
//!   proposer), and validate incoming proposals. These callbacks are
//!   the application's only signal that a block exists — the service
//!   no longer exposes a separate event stream.
//!
//! ## Strict ordering guarantees
//!
//! The service exists to keep the application out of the BFT
//! plumbing entirely. To make that possible it commits to:
//!
//! - `process_mb_proposal(block_hash, block)` is called as soon as a
//!   proposal has been assembled and validated, but only after every
//!   ancestor of `block_hash` has already returned successfully from
//!   a previous `process_mb_proposal` call;
//! - `process_mb_finalized(block_hash, cert)` is called only after
//!   `block_hash` was processed as a proposal and every ancestor was
//!   already finalized;
//! - `build_block_above` / `validate_block_above` are called only
//!   after the parent block is finalized (or `parent_hash == H256::zero()`
//!   for the genesis block).
//!
//! These invariants make the application a pure consumer of a
//! linearised block stream.
//!
//! The service's [`Stream`] impl carries only fatal app-task errors
//! (one terminating `anyhow::Error` per failure) — successful events
//! reach the application exclusively through the [`Externalities`]
//! callbacks.
//!
//! [`Stream`]: futures::Stream

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
