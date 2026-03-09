// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethexe_common::{
    HashOf,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedPromise,
    },
};
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "injected"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "injected"))]
pub trait Injected {
    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe",
        item = SignedPromise
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> SubscriptionResult;

    #[method(name = "getTransactionPromise")]
    async fn get_transaction_promise(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedPromise>>;
}
