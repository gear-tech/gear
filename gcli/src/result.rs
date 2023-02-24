//! Custom result

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    GSdk(#[from] gsdk::result::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("Invalid node key")]
    BadNodeKey,
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
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
    SubxtPublic(#[from] gsdk::ext::sp_core::crypto::PublicError),
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
