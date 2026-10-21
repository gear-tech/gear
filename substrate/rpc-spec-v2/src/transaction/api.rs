// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! API trait for transactions.

use crate::transaction::{error::ErrorBroadcast, event::TransactionEvent};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sp_core::Bytes;

#[rpc(client, server)]
pub trait TransactionApi<Hash: Clone> {
    /// Submit an extrinsic to watch.
    ///
    /// See [`TransactionEvent`] for details on
    /// transaction life cycle.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[subscription(
		name = "transactionWatch_v1_submitAndWatch" => "transactionWatch_v1_watchEvent",
		unsubscribe = "transactionWatch_v1_unwatch",
		item = TransactionEvent<Hash>,
	)]
    fn submit_and_watch(&self, bytes: Bytes);
}

#[rpc(client, server)]
pub trait TransactionBroadcastApi {
    /// Broadcast an extrinsic to the chain.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.

    #[method(name = "transaction_v1_broadcast", with_extensions)]
    async fn broadcast(&self, bytes: Bytes) -> RpcResult<Option<String>>;

    /// Broadcast an extrinsic to the chain.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "transaction_v1_stop", with_extensions)]
    async fn stop_broadcast(&self, operation_id: String) -> Result<(), ErrorBroadcast>;
}
