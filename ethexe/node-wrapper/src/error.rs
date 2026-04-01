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

/// Error type for the Vara.eth node wrapper.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No stdout was captured from the child process.
    #[error("no stdout was captured from the process")]
    NoStdout,
    /// Vara.eth binary not found in `$PATH`.
    #[error("Vara.eth binary not found in $PATH: {0}")]
    BinaryNotFound(#[source] which::Error),
    /// Failed to spawn Vara.eth node process.
    #[error("couldn't spawn node: {0}")]
    Spawn(#[source] std::io::Error),
    /// Timed out while waiting for the node to start.
    #[error("timed out waiting for node to spawn; is the node binary installed?")]
    Timeout,
    /// Failed to read the node output stream.
    #[error("could not read line from node output: {0}")]
    ReadLine(#[source] std::io::Error),
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
