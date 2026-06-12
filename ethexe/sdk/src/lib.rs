// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(dead_code)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # ethexe-sdk
//!
//! Rust client SDK for the Vara.ETH execution layer — Gear programs running on Ethereum via
//! ethexe. It bundles an ethexe JSON-RPC WebSocket client with an Ethereum contract client
//! behind a single [`VaraEthApi`] handle.
//!
//! This is a thin, `std`-only convenience layer for external consumers: it holds no execution,
//! consensus, or storage logic and delegates to `ethexe-ethereum`, `ethexe-rpc`, and
//! `ethexe-node-wrapper`. The primary in-workspace consumer is `ethexe-node-loader`, which
//! builds [`VaraEthApi`] clients for integration and fuzz testing.
//!
//! ## Public API
//!
//! - [`VaraEthApi`] — SDK root; built with `VaraEthApi::new` or [`VaraEthApi::builder`].
//!   Factory methods `mirror`, `router`, `wrapped_vara` return scoped wrappers.
//! - [`Mirror`] — Per-program operations: `send_message`, `send_reply`, `send_message_injected`, `wait_for_reply`, `claim_value`,
//!   `state`, `calculate_reply_for_handle`, plus `*_with_receipt` variants.
//! - [`Router`] — Router-contract and global queries: `request_code_validation`, `create_program`, validator queries,
//!   `code_state`, `program_ids`, `storage_view`.
//! - [`WVara`] — WrappedVara ERC20 queries and transfers, plus `mint` and `events`.
//! - [`types`] — SDK-visible result and value types.
//! - [`node_bindings`] — Bindings for spawning and managing a local ethexe node process.
//!
//! ## Usage example
//!
//! ```rust,no_run
//! use ethexe_sdk::VaraEthApi;
//!
//! // `eth_client` is an `ethexe_ethereum::Ethereum`; `rpc_url` is the node's WS endpoint.
//! let api = VaraEthApi::new(&rpc_url, eth_client).await?;
//!
//! let (_, code_id) = api.router().request_code_validation(wasm).await?;
//! api.router().wait_for_code_validation(code_id).await?;
//!
//! let (_, program_id) = api
//!     .router()
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
//! - [`Mirror`] and [`Router`] borrow `&VaraEthApi` and cannot outlive the handle they were
//!   created from.
//! - Injected transactions must carry zero value; a non-zero value is a hard error at call time.
//! - Most methods are `async` and return `anyhow::Result`, assuming a live RPC WebSocket and a
//!   reachable Ethereum endpoint.

pub use crate::{
    api::{
        DEFAULT_BLOB_GAS_MULTIPLIER, DEFAULT_EIP1559_FEE_INCREASE_PERCENTAGE,
        DEFAULT_EIP1559_MAX_FEE_PER_GAS_IN_GWEI, DEFAULT_ETHEREUM_RPC, VaraEthApi,
        VaraEthApiBuilder,
    },
    mirror::Mirror,
    router::Router,
    wvara::WVara,
};

mod api;
mod mirror;
pub mod node_bindings;
mod router;
pub mod types;
mod wvara;

pub use ethexe_ethereum::{Ethereum, EthereumBuilder};
