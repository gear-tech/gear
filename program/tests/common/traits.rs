//! Shared traits.

/// Convert self into `String`.
pub trait Convert<T> {
    fn convert(&self) -> T;
}

impl Convert<String> for Vec<u8> {
    fn convert(&self) -> String {
        String::from_utf8_lossy(self).to_string()
    }
}
