// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{
    HashOf,
    injected::{
        InjectedTransaction, SignedInjectedTransaction, SignedTxReceipt, Transaction,
        TransactionAcceptance,
    },
};
use gear_tdec::bls12_381::DkgPublicKey;
use jsonrpsee::proc_macros::rpc;

#[rpc(server, namespace = "injected")]
pub trait Injected {
    #[method(name = "getShieldingKey")]
    async fn shielding_key(&self) -> jsonrpsee::core::RpcResult<Option<DkgPublicKey>>;

    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: Transaction,
    ) -> jsonrpsee::core::RpcResult<TransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe",
        item = SignedTxReceipt
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: Transaction,
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
