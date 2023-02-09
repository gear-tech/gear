#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No stderr was found.")]
    EmptyStderr,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
