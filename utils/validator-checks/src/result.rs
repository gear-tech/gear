//! Errors and Result.

use std::result::Result as StdResult;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    GProgram(#[from] gp::result::Error),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
}

pub type Result<T> = StdResult<T, Error>;
