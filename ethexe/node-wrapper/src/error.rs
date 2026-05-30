// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/// Error type for the Vara.eth node wrapper.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // # Node Errors
    /// Vara.eth binary not found in `$PATH`.
    #[error("Vara.eth binary not found in $PATH: {0}")]
    BinaryNotFound(#[source] which::Error),
    /// Failed to spawn Vara.eth node process.
    #[error("couldn't spawn node: {0}")]
    Spawn(#[source] std::io::Error),
    /// Timed out while waiting for the node to start.
    #[error("timed out waiting for node to spawn; is the node binary installed?")]
    Timeout,

    // # Instance Errors
    /// Failed to build the Vara.eth HTTP client.
    #[error("failed to build HTTP client: {0}")]
    BuildHttpClient(#[source] jsonrpsee::core::ClientError),
    /// Failed to build the Vara.eth websocket client.
    #[error("failed to build websocket client: {0}")]
    BuildWsClient(#[source] jsonrpsee::core::ClientError),
    /// Failed to query the Ethereum router address.
    #[error("failed to query router address: {0}")]
    QueryRouterAddress(#[source] jsonrpsee::core::ClientError),
}
