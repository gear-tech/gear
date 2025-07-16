// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

pub mod calls;
pub mod error;
pub mod listener;
mod rpc;
pub mod storage;
pub mod voucher;

use crate::{EventListener, ws::WSAddress};
use error::*;
use gear_node_wrapper::{Node, NodeInstance};
use gsdk::{
    Api, ApiBuilder,
    ext::{sp_core::sr25519, sp_runtime::AccountId32},
    signer::Signer,
};
use std::{ffi::OsStr, sync::Arc};

/// The API instance contains methods to access the node.
#[derive(Clone)]
pub struct GearApi(Signer, Option<Arc<NodeInstance>>);

impl GearApi {
    /// Create api builder
    pub fn builder() -> GearApiBuilder {
        GearApiBuilder::default()
    }

    /// Create and init a new `GearApi` specified by its `address` on behalf of
    /// the default `Alice` user.
    pub async fn init(address: WSAddress) -> Result<Self> {
        Self::init_with(address, "//Alice").await
    }

    /// Create and init a new `GearApi` specified by its `address` and `suri`.
    ///
    /// SURI is a Substrate URI that identifies a user by a mnemonic phrase or
    /// provides default users from the keyring (e.g., "//Alice", "//Bob",
    /// etc.). The password for URI should be specified in the same `suri`,
    /// separated by the `':'` char.
    pub async fn init_with(address: WSAddress, suri: impl AsRef<str>) -> Result<Self> {
        Self::builder().suri(suri).build(address).await
    }

    /// Change SURI to the provided `suri` and return `Self`.
    pub fn with(self, suri: impl AsRef<str>) -> Result<Self> {
        let mut suri = suri.as_ref().splitn(2, ':');

        Ok(Self(
            self.0
                .change(suri.next().expect("Infallible"), suri.next())?,
            self.1,
        ))
    }

    /// Create and init a new `GearApi` instance that will be used with the
    /// local node working in developer mode (running with `--dev` argument).
    pub async fn dev() -> Result<Self> {
        Self::init(WSAddress::dev()).await
    }

    /// Create and init a new `GearApi` instance via spawning a new node process
    /// in development mode using the `--dev` flag and listening on an a
    /// random port number. The node process uses a binary specified by the
    /// `path` param. Ideally, the binary should be downloaded by means of CI pipeline from <https://get.gear.rs>.
    pub async fn dev_from_path(path: impl AsRef<OsStr>) -> Result<Self> {
        let node = Node::from_path(path.as_ref())?.spawn()?;
        let api = Self::init(node.address.into()).await?;
        Ok(Self(api.0, Some(Arc::new(node))))
    }

    /// Create and init a new `GearApi` instance via spawning a new node process
    /// in development mode using the `--chain=vara-dev`, `--validator`,
    /// `--tmp` flags and listening on an a random port number. The node
    /// process uses a binary specified by the `path` param. Ideally, the
    /// binary should be downloaded by means of CI pipeline from <https://get.gear.rs>.
    pub async fn vara_dev_from_path(path: impl AsRef<OsStr>) -> Result<Self> {
        let node = Node::from_path(path.as_ref())?.arg("--validator").spawn()?;
        let api = Self::init(node.address.into()).await?;
        Ok(Self(api.0, Some(Arc::new(node))))
    }

    /// Print node logs.
    pub fn print_node_logs(&mut self) {
        if let Some(node) = self.1.as_mut() {
            Arc::get_mut(node)
                .expect("Unable to mutate `Node`")
                .logs()
                .expect("Unable to read logs");
        }
    }

    /// Create and init a new `GearApi` instance that will be used with the
    /// public Vara testnet node.
    pub async fn vara_testnet() -> Result<Self> {
        Self::init(WSAddress::vara_testnet()).await
    }

    /// Create and init a new `GearApi` instance that will be used with the
    /// public Vara node.
    pub async fn vara() -> Result<Self> {
        Self::init(WSAddress::vara()).await
    }

    /// Create an [`EventListener`] to subscribe and handle continuously
    /// incoming events.
    pub async fn subscribe(&self) -> Result<EventListener> {
        let events = self.0.api().subscribe_finalized_blocks().await?;
        Ok(EventListener(events))
    }

    /// Set the number used once (`nonce`) that will be used while sending
    /// extrinsics.
    ///
    /// If set, this nonce is added to the extrinsic and provides a unique
    /// identifier of the transaction sent by the current account. The nonce
    /// shows how many prior transactions have occurred from this account. This
    /// helps protect against replay attacks or accidental double-submissions.
    pub fn set_nonce(&mut self, nonce: u64) {
        self.0.set_nonce(nonce)
    }

    /// Get the next number used once (`nonce`) from the node.
    ///
    /// Actually sends the `system_accountNextIndex` RPC to the node.
    pub async fn rpc_nonce(&self) -> Result<u64> {
        self.0
            .api()
            .tx()
            .account_nonce(self.0.account_id())
            .await
            .map_err(Into::into)
    }

    /// Return the signer account address.
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::{GearApi, Result};
    /// use gsdk::ext::sp_runtime::AccountId32;
    /// # use hex_literal::hex;
    ///
    /// #[tokio::test]
    /// async fn account_test() -> Result<()> {
    ///     let api = GearApi::dev().await?;
    ///     let alice_id = hex!("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d");
    ///     assert_eq!(api.account_id(), &AccountId32::from(alice_id));
    ///     Ok(())
    /// }
    /// ```
    pub fn account_id(&self) -> &AccountId32 {
        self.0.account_id()
    }
}

impl From<Signer> for GearApi {
    fn from(signer: Signer) -> Self {
        Self(signer, None)
    }
}

impl From<Api> for GearApi {
    fn from(api: Api) -> Self {
        Signer::new(api, "//Alice", None)
            .expect("//Alice always works")
            .into()
    }
}

impl From<(Api, sr25519::Pair)> for GearApi {
    fn from((api, signer): (Api, sr25519::Pair)) -> Self {
        Signer::from((api, signer.into())).into()
    }
}

impl From<GearApi> for Signer {
    fn from(api: GearApi) -> Self {
        api.0
    }
}

/// Gear API builder
pub struct GearApiBuilder {
    /// gsdk api builder
    inner: ApiBuilder,
    /// suri for keypair
    suri: String,
}

impl GearApiBuilder {
    /// Set retries of rpc client
    pub fn retries(mut self, retries: u8) -> Self {
        self.inner = self.inner.retries(retries);
        self
    }

    /// Set timeout of rpc client ( in milliseconds )
    pub fn timeout(mut self, timeout: u64) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Set initial suri for keypiar
    pub fn suri(mut self, suri: impl AsRef<str>) -> Self {
        self.suri = suri.as_ref().into();
        self
    }

    /// Build gear api
    pub async fn build(self, address: WSAddress) -> Result<GearApi> {
        let mut suri = self.suri.splitn(2, ':');
        let api = self.inner.build(address.url().as_str()).await?;

        Ok(GearApi(
            api.signer(suri.next().expect("Infallible"), suri.next())?,
            None,
        ))
    }
}

impl Default for GearApiBuilder {
    fn default() -> Self {
        Self {
            suri: "//Alice".into(),
            inner: ApiBuilder::default(),
        }
    }
}
