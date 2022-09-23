//! Metadata result

/// Metadata error
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Memory not exists")]
    MemoryNotExists,
    #[error("Metadata {0} not exists")]
    MetadataNotExists(String),
    #[error("Type {0} not found")]
    TypeNotFound(String),
    #[error("Type registry not found")]
    RegistryNotFound,
    #[error("Read {0} failed")]
    ReadMetadataFailed(String),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Codec(#[from] parity_scale_codec::Error),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
}

/// Metadata result
pub type Result<T> = std::result::Result<T, Error>;
