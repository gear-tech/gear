// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(dead_code)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # ethexe-sdk
//!
//! Rust client SDK for the Vara.ETH execution layer — Gear programs running on Ethereum via ethexe.
//! It provides a typed, async facade that bundles an ethexe JSON-RPC WebSocket client with an
//! Ethereum contract client behind a single [`VaraEthApi`] handle.
//!
//! ## Purpose
//!
//! `ethexe-sdk` is a thin convenience layer for external consumers. It contains no execution,
//! consensus, or storage logic; every method delegates to `ethexe-ethereum`, `ethexe-rpc`, or
//! `ethexe-node-wrapper`. The crate is `std`-only.
//!
//! ## Role in the stack
//!
//! ```text
//! Consumer / Integration test
//!         │
//!         ▼
//!   ethexe-sdk  (VaraEthApi / Mirror / Router / WVara)
//!    ╱                           ╲
//! ethexe-rpc (client feature)    ethexe-ethereum
//!   JSON-RPC WebSocket              Router, Mirror,
//!   (state, reply calc,             WVara contract
//!    injected txs)                  clients
//!         │                               │
//!         └─────────── Ethereum ──────────┘
//!                   on-chain contracts
//! ```
//!
//! `ethexe-node-loader` is the primary in-workspace consumer; it builds [`VaraEthApi`] clients for
//! integration and fuzz testing.
//!
//! ## Entry points / Public API
//!
//! - [`VaraEthApi`] — SDK root; constructed with `VaraEthApi::new(rpc_url, ethereum_client)`.
//!   Factory methods return scoped wrappers: `mirror(actor_id)`, `router()`, `wrapped_vara()`.
//! - [`Mirror`] — per-program operations: `send_message`, `send_reply`, `send_message_injected`,
//!   `wait_for_reply`, `claim_value`, `state`, `calculate_reply_for_handle`, and `*_with_receipt`
//!   variants that return the alloy `TransactionReceipt`.
//! - [`Router`] — router-contract and global queries: `request_code_validation`,
//!   `wait_for_code_validation`, `create_program`, validator queries (`validators`, `is_validator`,
//!   `validators_threshold`), `code_state`, `program_ids`, `storage_view`.
//! - [`WVara`] — WrappedVara ERC20 operations: standard token queries and transfers, plus `mint` and
//!   `events()`.
//! - [`VaraEth`], [`VaraEthInstance`], [`Error`] — re-exported from `ethexe-node-wrapper`; spawn and
//!   manage a local ethexe node process and obtain its RPC endpoints.
//!
//! ## Usage example
//!
//! ```rust,no_run
//! use ethexe_sdk::VaraEthApi;
//!
//! // `eth_client` is an `ethexe_ethereum::Ethereum`; `rpc_url` is the node's WS endpoint.
//! let api = VaraEthApi::new(&rpc_url, eth_client).await?;
//!
//! // Upload and validate code on-chain.
//! let (_, code_id) = api.router().request_code_validation(wasm).await?;
//! api.router().wait_for_code_validation(code_id).await?;
//!
//! // Create a program and interact with it.
//! let (_, program_id) = api.router()
//!     .create_program_with_executable_balance(code_id, salt, None, balance)
//!     .await?;
//! let mirror = api.mirror(program_id);
//! let (_, message_id) = mirror.send_message(payload, value).await?;
//! let reply = mirror.wait_for_reply(message_id).await?;
//! # anyhow::Ok(())
//! ```
//!
//! ## Invariants
//!
//! - [`Mirror`] and [`Router`] borrow `&VaraEthApi` and cannot outlive the handle they were created
//!   from.
//! - Injected transactions must carry zero value; a non-zero value is a hard error at call time.
//! - `send_message_injected` treats `Accept` and `AlreadyPooled` responses as success; `Reject` is
//!   returned as an error.
//! - Most methods are `async` and return `anyhow::Result`, assuming a live RPC WebSocket and a
//!   reachable Ethereum endpoint.

pub use crate::{api::VaraEthApi, mirror::Mirror, router::Router, wvara::WVara};

mod api;
mod mirror;
mod router;
mod wvara;

// Re-export the
pub use ethexe_node_wrapper::{Error, VaraEth, VaraEthInstance};
