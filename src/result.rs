/// transaction error
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("transaction retracted {0}")]
    Retracted(String),
    #[error("transaction timeout {0}")]
    FinalityTimeout(String),
    #[error("transaction usurped {0}")]
    Usurped(String),
    #[error("transaction dropped")]
    Dropped,
    #[error("transaction invalid")]
    Invalid,
}

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not find directory {0}")]
    CouldNotFindDirectory(String),
    #[error("InvalidSecret")]
    InvalidSecret,
    #[error("No available account was found in keystore, please run `gear login` first.")]
    Logout,
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SubxtBasic(#[from] subxt::BasicError),
    #[error(transparent)]
    SubxtGeneric(
        #[from]
        subxt::GenericError<
            subxt::RuntimeError<
                crate::api::generated::api::runtime_types::sp_runtime::DispatchError,
            >,
        >,
    ),
    #[error(transparent)]
    SubxtRpc(#[from] subxt::rpc::RpcError),
    #[error(transparent)]
    Tx(#[from] TransactionError),
}

/// Custom result
pub type Result<T> = std::result::Result<T, Error>;
