//! Utility module.
use anyhow::Result;

use alloc::string::String;
use core::fmt::Write;

pub fn encode_hex(bytes: &[u8]) -> Result<String> {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b)?
    }
    Ok(s)
}
