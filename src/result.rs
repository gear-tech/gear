/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not find directory {0}")]
    CouldNotFindDirectory(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Custom result
pub type Result<T> = std::result::Result<T, Error>;
