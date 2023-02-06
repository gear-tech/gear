//! Errors and Result.

use std::result::Result as StdResult;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Some errors happens in gprogram.
    #[error(transparent)]
    GProgram(#[from] gp::result::Error),
    /// Some errors happens in subxt.
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    /// Failed to setup logger.
    #[error(transparent)]
    EnvLogger(#[from] log::SetLoggerError),
    /// Decoding ss58 address failed.
    #[error(transparent)]
    PublicError(#[from] subxt::ext::sp_core::crypto::PublicError),
    /// Blocks production validation failed.
    #[error("Some validators didn't produce blocks.")]
    BlocksProduction,
}

pub type Result<T> = StdResult<T, Error>;
