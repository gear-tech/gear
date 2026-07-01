// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::types::PromiseSubscriptionFilter;
use ethexe_common::{
    HashOf,
    injected::{
        InjectedTransaction, InjectedTransactionAcceptance, SignedInjectedTransaction,
        SignedTxReceipt,
    },
};
use jsonrpsee::proc_macros::rpc;

#[rpc(client, namespace = "injected")]
pub trait Injected {
    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: SignedInjectedTransaction,
    ) -> jsonrpsee::core::RpcResult<InjectedTransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe",
        item = SignedTxReceipt
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: SignedInjectedTransaction,
    ) -> jsonrpsee::core::SubscriptionResult;

    /// Subscribes to a stream of all newly computed promises. Promises are
    /// delivered as they are computed; there is no replay of historical
    /// promises. An optional filter narrows the stream per subscriber.
    #[subscription(
        name = "subscribePromises",
        unsubscribe = "unsubscribePromises",
        item = crate::types::PromiseEnvelope
    )]
    async fn subscribe_promises(
        &self,
        filter: Option<PromiseSubscriptionFilter>,
    ) -> jsonrpsee::core::SubscriptionResult;

    #[method(name = "getTransactionReceipt")]
    async fn get_transaction_receipt(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> jsonrpsee::core::RpcResult<Option<SignedTxReceipt>>;

    /// Retrieves injected transactions by the provided IDs.
    #[method(name = "getTransactions")]
    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> jsonrpsee::core::RpcResult<Vec<Option<SignedInjectedTransaction>>>;
}
