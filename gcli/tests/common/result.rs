#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No stderr was found.")]
    EmptyStderr,
    #[error(transparent)]
    GCli(#[from] gcli::result::Error),
    #[error(transparent)]
    GSdk(#[from] gsdk::result::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
