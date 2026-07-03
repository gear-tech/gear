// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{BlockHeader, events::BlockRequestEvent};
use gprimitives::H256;
use jsonrpsee::proc_macros::rpc;

#[rpc(client)]
pub trait Block {
    #[method(name = "block_header")]
    async fn block_header(
        &self,
        hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<(H256, BlockHeader)>;

    #[method(name = "block_events")]
    async fn block_events(
        &self,
        block_hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<Vec<BlockRequestEvent>>;
}
