//! Gear program utils

use crate::result::Result;
use anyhow::anyhow;
use std::{fs, path::PathBuf};

/// home directory of cli `gear`
pub fn home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into()).join(".gear");

    if !home.exists() {
        fs::create_dir_all(&home).expect("Failed to create ~/.gear");
    }

    home
}

pub fn hex_to_vec(string: impl AsRef<str>) -> Result<Vec<u8>> {
    hex::decode(string.as_ref().trim_start_matches("0x")).map_err(Into::into)
}

pub fn hex_to_hash(string: impl AsRef<str>) -> Result<[u8; 32]> {
    let hex = hex_to_vec(string)?;

    if hex.len() != 32 {
        return Err(anyhow!("Incorrect id length").into());
    }

    let mut arr = [0; 32];
    arr.copy_from_slice(&hex);

    Ok(arr)
}
