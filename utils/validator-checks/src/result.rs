//! Errors and Result.

use std::result::Result as StdResult;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    GProgram(#[from] gp::result::Error),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    EnvLogger(#[from] log::SetLoggerError),
}

pub type Result<T> = StdResult<T, Error>;
