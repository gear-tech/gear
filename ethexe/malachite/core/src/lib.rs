// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

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
//! - [`Externalities`] — async callbacks the service invokes to save
//!   blocks, mark them finalized, build new ones (when proposer),
//!   and validate incoming proposals;
//! - [`MalachiteEvent`] — outbound notifications surfaced through
//!   the service's [`Stream`] impl.
//!
//! ## Strict ordering guarantees
//!
//! The service exists to keep the application out of the BFT
//! plumbing entirely. To make that possible it commits to:
//!
//! - `save_block(block_hash, block)` is called only after every
//!   ancestor of `block_hash` has been saved successfully;
//! - `mark_block_as_finalized(block_hash, cert)` is called only after
//!   `block_hash` was saved and every ancestor was already finalized;
//! - `build_block_above` / `validate_block_above` are called only
//!   after the parent block is finalized (or `parent_hash == H256::zero()`
//!   for the genesis block).
//!
//! These invariants make the application a pure consumer of a
//! linearised block stream.
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
    types::{Address, Block, CommitCertificate, H256, MalachiteEvent},
};

/// Re-exported libp2p PeerId — used by integration tests / operators
/// to materialize `/p2p/<peer-id>` multiaddr suffixes.
pub use libp2p_identity::PeerId;
