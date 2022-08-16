#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No stderr was found.")]
    EmptyStderr,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Spwan process failed: {0}")]
    Spawn(String),
}

pub type Result<T> = std::result::Result<T, Error>;
