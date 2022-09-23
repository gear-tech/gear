#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No stderr was found.")]
    EmptyStderr,
    #[error(transparent)]
    GearProgram(#[from] gear_program::result::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
