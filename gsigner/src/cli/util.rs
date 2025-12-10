// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Common CLI utilities shared by handlers and display code.

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
use crate::address::SubstrateAddress;
use crate::cli::commands::StorageLocationArgs;
use anyhow::{Result, anyhow};
use std::path::PathBuf;

pub fn prefixed_message(data_hex: &str, prefix: &Option<String>) -> Result<Vec<u8>> {
    let max_len = MAX_MESSAGE_BYTES
        .checked_mul(2)
        .ok_or_else(|| anyhow!("message length limit overflow"))?;
    if data_hex.len() > max_len {
        anyhow::bail!("Data is too large: exceeds {MAX_MESSAGE_BYTES} bytes");
    }

    let mut message = Vec::new();

    if let Some(prefix) = prefix {
        message.extend_from_slice(prefix.as_bytes());
    }

    let data_bytes = hex::decode(data_hex)?;
    message.extend_from_slice(&data_bytes);

    Ok(message)
}

pub fn strip_0x(value: &str) -> &str {
    value.strip_prefix("0x").unwrap_or(value)
}

/// Validate and normalise a fixed-length hex string.
pub fn hex_bytes<const N: usize>(input: &str) -> Result<String, String> {
    let trimmed = strip_0x(input);
    if trimmed.len() != N * 2 {
        return Err(format!("expected {N}-byte hex ({} chars)", N * 2));
    }

    if trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(trimmed.to_string())
    } else {
        Err("invalid hex string".to_string())
    }
}

pub fn validate_hex_len(value: &str, expected_bytes: usize, label: &str) -> Result<()> {
    let normalized = strip_0x(value);
    let expected_len = expected_bytes * 2;
    if normalized.len() != expected_len {
        anyhow::bail!("Invalid {label}: expected {expected_bytes}-byte hex");
    }
    Ok(())
}

pub fn decode_hex_array<const N: usize>(hex_str: &str, label: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() != N {
        anyhow::bail!("Invalid {label} length: expected {N} bytes");
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub fn default_storage_root() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gsigner")
}

pub fn storage_root(path: &Option<PathBuf>) -> PathBuf {
    path.clone().unwrap_or_else(default_storage_root)
}

pub fn resolve_storage_location(args: &StorageLocationArgs) -> Option<PathBuf> {
    if args.memory {
        None
    } else {
        Some(storage_root(&args.path))
    }
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
pub fn substrate_address_display(address: &SubstrateAddress) -> String {
    address.as_ss58().to_string()
}

pub const MAX_MESSAGE_BYTES: usize = 1_048_576; // 1 MiB limit for CLI inputs

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn prefixed_message_combines_prefix_and_data() {
        let msg = prefixed_message("c0ffee", &Some("pre".to_string())).unwrap();
        assert_eq!(msg, b"pre\xC0\xFF\xEE");
    }

    #[test]
    fn prefixed_message_handles_empty_prefix() {
        let msg = prefixed_message("00", &None).unwrap();
        assert_eq!(msg, vec![0u8]);
    }

    #[test]
    fn prefixed_message_rejects_large_payload() {
        let oversized_hex = "aa".repeat(MAX_MESSAGE_BYTES + 1);
        let err = prefixed_message(&oversized_hex, &None).unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn strip_0x_removes_prefix() {
        assert_eq!(strip_0x("0xabc"), "abc");
        assert_eq!(strip_0x("abc"), "abc");
    }

    #[test]
    fn validate_hex_len_accepts_exact_length() {
        validate_hex_len("0a0b", 2, "test").expect("expected valid hex length");
    }

    #[test]
    fn validate_hex_len_rejects_wrong_length() {
        let err = validate_hex_len("0a0b0c", 2, "test").unwrap_err();
        assert!(err.to_string().contains("expected 2-byte hex"));
    }

    #[cfg(any(feature = "ed25519", feature = "sr25519"))]
    #[test]
    fn decode_hex_array_reads_exact_length() {
        let bytes = decode_hex_array::<2>("0a0b", "test").unwrap();
        assert_eq!(bytes, [0x0a, 0x0b]);
    }

    #[cfg(any(feature = "ed25519", feature = "sr25519"))]
    #[test]
    fn decode_hex_array_errors_on_wrong_length() {
        let err = decode_hex_array::<2>("0a0b0c", "test").unwrap_err();
        assert!(err.to_string().contains("expected 2 bytes"));
    }

    #[test]
    fn resolve_storage_location_honors_memory_flag() {
        let args = StorageLocationArgs {
            memory: true,
            path: None,
            storage_password: None,
        };
        assert!(resolve_storage_location(&args).is_none());
    }

    #[test]
    fn resolve_storage_location_uses_custom_path() {
        let args = StorageLocationArgs {
            memory: false,
            path: Some(PathBuf::from("custom")),
            storage_password: None,
        };
        assert_eq!(
            resolve_storage_location(&args),
            Some(PathBuf::from("custom"))
        );
    }
}
