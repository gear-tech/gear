// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate mixnet API.

use jsonrpsee::core::async_trait;
use sc_mixnet::Api;
use sc_rpc_api::mixnet::error::Error;
pub use sc_rpc_api::mixnet::MixnetApiServer;
use sp_core::Bytes;

/// Mixnet API.
pub struct Mixnet(futures::lock::Mutex<Api>);

impl Mixnet {
    /// Create a new mixnet API instance.
    pub fn new(api: Api) -> Self {
        Self(futures::lock::Mutex::new(api))
    }
}

#[async_trait]
impl MixnetApiServer for Mixnet {
    async fn submit_extrinsic(&self, extrinsic: Bytes) -> Result<(), Error> {
        // We only hold the lock while pushing the request into the requests channel
        let fut = {
            let mut api = self.0.lock().await;
            api.submit_extrinsic(extrinsic).await
        };
        Ok(fut.await.map_err(Error)?)
    }
}
