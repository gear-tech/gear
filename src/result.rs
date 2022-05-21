/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not find directory {0}")]
    CouldNotFindDirectory(String),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SubxtBasic(#[from] subxt::BasicError),
    #[error(transparent)]
    SubxtRpc(#[from] subxt::rpc::RpcError),
}

/// Custom result
pub type Result<T> = std::result::Result<T, Error>;
