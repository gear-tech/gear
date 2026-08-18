// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Errors that can occur during the service operation.

use sc_keystore;
use sp_blockchain;
use sp_consensus;

/// Service Result typedef.
pub type Result<T> = std::result::Result<T, Error>;

/// Service errors.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Client(#[from] sp_blockchain::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Consensus(#[from] sp_consensus::Error),

    #[error(transparent)]
    Network(#[from] sc_network::error::Error),

    #[error(transparent)]
    Keystore(#[from] sc_keystore::Error),

    #[error(transparent)]
    Telemetry(#[from] sc_telemetry::Error),

    #[error("Best chain selection strategy (SelectChain) is not provided.")]
    SelectChainRequired,

    #[error("Tasks executor hasn't been provided.")]
    TaskExecutorRequired,

    #[error("Prometheus metrics error: {0}")]
    Prometheus(#[from] prometheus_endpoint::PrometheusError),

    #[error("Application: {0}")]
    Application(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Other: {0}")]
    Other(String),
}

impl<'a> From<&'a str> for Error {
    fn from(s: &'a str) -> Self {
        Error::Other(s.into())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}
