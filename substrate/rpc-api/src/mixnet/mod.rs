// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate mixnet API.

pub mod error;

use error::Error;
use jsonrpsee::proc_macros::rpc;
use sp_core::Bytes;

#[rpc(client, server)]
pub trait MixnetApi {
    /// Submit encoded extrinsic over the mixnet for inclusion in block.
    #[method(name = "mixnet_submitExtrinsic")]
    async fn submit_extrinsic(&self, extrinsic: Bytes) -> Result<(), Error>;
}
