use crate::api::{
    config::GearConfig,
    generated::api::{runtime_types::sp_runtime::DispatchError, Event},
};
use subxt::{sp_core::H256, TransactionStatus};

type TxStatus<'t> = TransactionStatus<'t, GearConfig, DispatchError, Event>;

/// transaction error
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("Transaction Retracted( {0} )")]
    Retracted(H256),
    #[error("Transaction Timeout( {0} )")]
    FinalityTimeout(H256),
    #[error("Transaction Usurped( {0} )")]
    Usurped(H256),
    #[error("Transaction Dropped")]
    Dropped,
    #[error("Transaction Invalid")]
    Invalid,
    #[error("Not an error, this will never be reached.")]
    None,
}

impl From<TxStatus<'_>> for Error {
    fn from(status: TxStatus<'_>) -> Self {
        match status {
            TransactionStatus::Retracted(h) => TransactionError::Retracted(h),
            TransactionStatus::FinalityTimeout(h) => TransactionError::FinalityTimeout(h),
            TransactionStatus::Usurped(h) => TransactionError::Usurped(h),
            _ => TransactionError::None,
        }
        .into()
    }
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
    Codec(#[from] parity_scale_codec::Error),
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
    SubxtMetadata(#[from] subxt::MetadataError),
    #[error(transparent)]
    SubxtPublic(#[from] subxt::sp_core::crypto::PublicError),
    #[error(transparent)]
    SubxtRpc(#[from] subxt::rpc::RpcError),
    #[error(transparent)]
    Tx(#[from] TransactionError),
}

/// Custom result
pub type Result<T> = std::result::Result<T, Error>;
