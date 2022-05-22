//! gear program utils
use std::path::PathBuf;

/// gear home
pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| ".".into()).join(".gear")
}
