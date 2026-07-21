// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # RPC Server Injected API
//!
//! ## Promises Flow
//!
//! A promise is the full reply that an included
//! [`InjectedTransaction`](ethexe_common::injected::InjectedTransaction) is expected to produce.
//! A receipt is the signed validator statement returned to RPC users. The RPC user always receives
//! [`SignedTxReceipt`](ethexe_common::injected::SignedTxReceipt), but validators gossip the lighter
//! [`SignedCompactTxReceipt`](ethexe_common::injected::SignedCompactTxReceipt): for a successful
//! transaction it contains only [`CompactPromise`](ethexe_common::injected::CompactPromise), and for
//! purged transaction it contains [`PurgedTransaction`](ethexe_common::injected::PurgedTransaction).
//!
//! [`promise_manager::PromiseSubscriptionManager`] owns the RPC-side joining logic. It keeps:
//! - one `watch` channel per transaction hash, fanning the receipt out to any number of
//!   concurrent watchers (subscribers are anonymous receiver clones, no per-subscriber id);
//! - full promises already computed locally and stored in the database;
//! - compact promise receipts whose full promise body has not been observed yet.
//!
//! ### Subscription Setup
//!
//! [`InjectedApi::send_transaction_and_watch`](server::InjectedApi::send_transaction_and_watch)
//! first checks whether a receipt is already stored for the transaction hash. If so, it accepts
//! the subscription and delivers the cached result immediately without relaying (the `Ready` path).
//! Otherwise it registers a new pending subscriber and relays the transaction; the relayer shares
//! a single in-flight relay per transaction hash, so concurrent watchers (and plain
//! `injected_sendTransaction` callers) of one transaction produce one relay and observe the same
//! Accept/Reject outcome. Once accepted, [`spawner::spawn_pending_subscriber`] waits for the
//! receipt, which is fanned out to every registered watcher; a late watcher whose receipt is
//! already stored is served immediately from the database.
//!
//! **Important:** the pending subscriber is dropped after **20 * Ethereum slot** seconds to avoid
//! dead subscribers. A later receipt can still be stored in the database and returned by
//! `injected_getTransactionReceipt`.
//!
//! ### Success Path
//!
//! 1. The selected producer includes the injected transaction into an announce.
//! 2. Compute executes the announce with promise emission enabled. When the injected message sends
//!    its reply, the runtime builds a full
//!    [`Promise`](ethexe_common::injected::Promise) from the reply and transaction hash.
//! 3. The service passes the full promise to RPC through
//!    [`promise_manager::PromiseSubscriptionManager::on_computed_promise`]. RPC stores it in the
//!    database.
//! 4. Consensus signs
//!    [`Receipt::Promise(promise.to_compact())`](ethexe_common::injected::Receipt::Promise) and
//!    emits the signed compact receipt. The service delivers it locally to RPC and gossips it over
//!    the network.
//! 5. [`promise_manager::PromiseSubscriptionManager::on_tx_receipt`] receives the compact receipt.
//!    If the full promise is already known, RPC checks that `promise.to_compact()` matches the
//!    signed compact promise, rebuilds the full signed receipt while preserving the producer
//!    signature, stores it, and sends it to the subscriber. If the compact receipt arrives first,
//!    RPC keeps the unfilled receipt until
//!    [`on_computed_promise`](promise_manager::PromiseSubscriptionManager::on_computed_promise)
//!    receives the matching full promise.
//!
//! ### Error Path
//!
//! Some transactions never execute and therefore have no promise body. When the producer purges
//! such a transaction from the pool, it signs
//! [`Receipt::Purged`](ethexe_common::injected::Receipt::Purged) with a
//! [`TransactionPurgedReason`](ethexe_common::injected::TransactionPurgedReason), currently
//! `Outdated`, `UnknownReferenceBlock` or `NonZeroValue`. `Receipt::Purged` upgrades to a full
//! [`SignedTxReceipt`](ethexe_common::injected::SignedTxReceipt) immediately because it does not
//! depend on a full promise. RPC stores the receipt and sends it to the subscriber.
//!
//! Other non-ready states, such as an unknown destination, uninitialized destination, insufficient
//! executable balance, duplicate transaction, or reference block on another branch, keep the
//! transaction in the pool and do not produce an error receipt immediately. The active watcher may
//! time out before a later block either includes or removes the transaction.
//!
//! If a compact promise receipt and the locally computed full promise disagree, RPC logs a warning
//! and keeps waiting for a matching computed promise. No signed full receipt is stored or delivered
//! for that mismatch.

pub(crate) mod promise_manager;

pub(crate) mod relay;

pub(crate) mod server;
pub use server::InjectedApi;

pub(crate) mod spawner;

mod r#trait;
pub use r#trait::InjectedServer;
