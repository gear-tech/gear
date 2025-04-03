//! Errors and Result.

use std::result::Result as StdResult;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Any error
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    /// Some errors happens in gprogram.
    #[error(transparent)]
    GSdk(#[from] gsdk::Error),
    /// Some errors happens in subxt.
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    /// Subxt core error
    #[error(transparent)]
    SubxtCore(#[from] subxt::ext::subxt_core::Error),
    /// Failed to parse env filter.
    #[error(transparent)]
    EnvFilter(#[from] tracing_subscriber::filter::ParseError),
    /// Decoding ss58 address failed.
    #[error(transparent)]
    PublicKey(#[from] gsdk::ext::sp_core::crypto::PublicError),
    /// Blocks production validation failed.
    #[error("Some validators didn't produce blocks.")]
    BlocksProduction,
}

pub type Result<T, E = Error> = StdResult<T, E>;
