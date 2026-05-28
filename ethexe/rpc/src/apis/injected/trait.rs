// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{
    HashOf,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedTxReceipt,
    },
};
use jsonrpsee::proc_macros::rpc;

#[cfg_attr(
    all(feature = "server", feature = "client"),
    rpc(server, client, namespace = "injected")
)]
#[cfg_attr(
    all(feature = "server", not(feature = "client")),
    rpc(server, namespace = "injected")
)]
#[cfg_attr(
    all(not(feature = "server"), feature = "client"),
    rpc(client, namespace = "injected")
)]
pub trait Injected {
    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> jsonrpsee::core::RpcResult<InjectedTransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe",
        item = SignedTxReceipt
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> jsonrpsee::core::SubscriptionResult;

    #[method(name = "getTransactionReceipt")]
    async fn get_transaction_receipt(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> jsonrpsee::core::RpcResult<Option<SignedTxReceipt>>;

    /// Retrieves injected transactions by the provided IDs
    #[method(name = "getTransactions")]
    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> jsonrpsee::core::RpcResult<Vec<Option<SignedInjectedTransaction>>>;
}
