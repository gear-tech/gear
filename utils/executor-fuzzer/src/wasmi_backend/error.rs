use derive_more::Display;
use wasmi::HostError;

#[derive(Debug, Display)]
#[display(fmt = "{message}")]
pub struct CustomHostError {
    message: String,
}

impl HostError for CustomHostError {}

impl<T> From<T> for CustomHostError
where
    T: Into<String>,
{
    fn from(s: T) -> CustomHostError {
        Self { message: s.into() }
    }
}
