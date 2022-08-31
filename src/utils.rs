//! Gear program utils
#![cfg(feature = "cli")]
use std::{fs, path::PathBuf};

/// home directory of cli `gear`
pub fn home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into()).join(".gear");

    if !home.exists() {
        fs::create_dir_all(&home).expect("Failed to create ~/.gear");
    }

    home
}
