//! Custom result

use crate::api::types::TxStatus;
use subxt::ext::sp_core::H256;

/// transaction error
#[derive(Debug, thiserror::Error)]
pub enum TxError {
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

impl From<TxStatus> for Error {
    fn from(status: TxStatus) -> Self {
        match status {
            TxStatus::Retracted(h) => TxError::Retracted(h),
            TxStatus::FinalityTimeout(h) => TxError::FinalityTimeout(h),
            TxStatus::Usurped(h) => TxError::Usurped(h),
            _ => TxError::None,
        }
        .into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Queried event not found.")]
    EventNotFound,
    #[error("Invalid rpc URL.")]
    InvalidUrl,
    #[error("Page {0} of Program {1} was not found in the storage.")]
    PageNotFound(u32, String),
    #[error("Program with id {0} was not found in the storage.")]
    ProgramNotFound(String),
    #[error("Program has been terminated.")]
    ProgramTerminated,
    #[error("The queried storage not found.")]
    StorageNotFound,
    #[error(transparent)]
    SubxtRpc(#[from] jsonrpsee::core::Error),
}

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("Invalid node key")]
    BadNodeKey,
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Codec(#[from] parity_scale_codec::Error),
    #[error("Code not found {0}")]
    CodeNotFound(String),
    #[error("Could not find directory {0}")]
    CouldNotFindDirectory(String),
    #[error("Event not found")]
    EventNotFound,
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error("Unable to get the name of the current executable binary")]
    InvalidExecutable,
    #[error("Password must be provided for logining with json file.")]
    InvalidPassword,
    #[error("Invalid public key")]
    InvalidPublic,
    #[error("Invalid secret key")]
    InvalidSecret,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Logger(#[from] log::SetLoggerError),
    #[error("No available account was found in keystore, please run `gear login` first.")]
    Logout,
    #[error(transparent)]
    Metadata(#[from] crate::metadata::Error),
    #[error("{0}")]
    Nacl(String),
    #[error("{0}")]
    Schnorrkel(schnorrkel::SignatureError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    SubxtPublic(#[from] subxt::ext::sp_core::crypto::PublicError),
    #[error(transparent)]
    SubxtMetadata(#[from] subxt::error::MetadataError),
    #[error(transparent)]
    Tx(#[from] TxError),
}

impl From<nacl::Error> for Error {
    fn from(err: nacl::Error) -> Self {
        Self::Nacl(err.message)
    }
}

impl From<schnorrkel::SignatureError> for Error {
    fn from(err: schnorrkel::SignatureError) -> Self {
        Self::Schnorrkel(err)
    }
}

/// Custom result
pub type Result<T> = std::result::Result<T, Error>;
