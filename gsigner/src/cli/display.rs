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

//! Display utilities for CLI output formatting.
//!
//! This module provides functions to format CLI results with colored output.

use super::handlers::*;
use colored::Colorize;

/// Display a command result with colored formatting
pub fn display_result(result: &CommandResult) {
    match result {
        CommandResult::Secp256k1(r) => display_secp256k1_result(r),
        CommandResult::Ed25519(r) => display_ed25519_result(r),
        CommandResult::Sr25519(r) => display_sr25519_result(r),
    }
}

fn address_label(address: &str) -> &'static str {
    if address.starts_with("0x") {
        "Address"
    } else {
        "SS58"
    }
}

#[cfg(feature = "keyring")]
fn display_keyring_result(result: &KeyringResult) {
    println!("{}", format!("✓ {}", result.message).green().bold());
    if let Some(details) = &result.details {
        println!(
            "  {} {}",
            "Key name:".bright_blue(),
            details.name.bright_white()
        );
        println!("  {} {}", "Public key:".bright_blue(), details.public_key);
        println!(
            "  {} {}",
            format!("{}:", address_label(&details.address)).bright_blue(),
            details.address
        );
        println!("  {} {}", "Scheme:".bright_blue(), details.scheme.as_str());
        if let Some(key_type) = &details.key_type {
            println!("  {} {}", "Key type:".bright_blue(), key_type);
        }
        if let Some(private_key) = &details.private_key {
            println!("  {} {}", "Private key:".bright_red(), private_key);
        }
        if let Some(keystore_name) = &details.keystore_name {
            println!("  {} {}", "Keystore:".bright_blue(), keystore_name);
        }
    }
}

#[cfg(feature = "keyring")]
fn display_keyring_list(result: &KeyringListResult) {
    if result.keystores.is_empty() {
        println!("{}", "No keys in keyring".yellow());
    } else {
        println!(
            "{}",
            format!("Keyring contains {} key(s):", result.keystores.len())
                .green()
                .bold()
        );
        for ks in &result.keystores {
            println!("  {} {}", "•".bright_blue(), ks.name.bright_white().bold());
            if let Some(public_key) = &ks.public_key {
                println!("    {} {}", "Public key:".bright_black(), public_key);
            }
            println!(
                "    {} {}",
                format!("{}:", address_label(&ks.address)).bright_black(),
                ks.address
            );
            println!("    {} {}", "Created:".bright_black(), ks.created);
            println!("    {} {}", "Scheme:".bright_black(), ks.scheme);
            if let Some(key_type) = &ks.key_type {
                println!("    {} {}", "Key type:".bright_black(), key_type);
            }
        }
    }
}

pub fn display_secp256k1_result(result: &Secp256k1Result) {
    match result {
        Secp256k1Result::Clear(r) => {
            println!("{} Removed {} key(s)", "✓".green().bold(), r.removed);
        }
        Secp256k1Result::Generate(r) => {
            println!("{}", "✓ Generated secp256k1 keypair".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "Address:".bright_blue(), r.address);
            println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
            println!("  {} {}", "Key type:".bright_blue(), r.key_type);
            if let Some(secret) = &r.secret {
                println!("  {} {}", "Secret:".red().bold(), secret);
            }
        }
        Secp256k1Result::Sign(r) => {
            println!("{}", "✓ Signed data".green().bold());
            println!("  {} {}", "Signature:".bright_blue(), r.signature);
        }
        Secp256k1Result::Verify(_) => {
            println!("{}", "✓ Signature is valid".green().bold());
        }
        Secp256k1Result::Recover(r) => {
            println!("{}", "✓ Recovered public key".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "Address:".bright_blue(), r.address);
        }
        Secp256k1Result::Address(r) => {
            println!("{} {}", "Address:".bright_blue(), r.address);
        }
        #[cfg(feature = "peer-id")]
        Secp256k1Result::PeerId(r) => {
            println!("{} {}", "PeerId:".bright_blue(), r.peer_id);
        }
        Secp256k1Result::List(r) => {
            if r.keys.is_empty() {
                println!("{}", "No keys found".yellow());
            } else {
                println!(
                    "{}",
                    format!("Found {} key(s):", r.keys.len()).green().bold()
                );
                for key in &r.keys {
                    println!("  {} {}", "•".bright_blue(), key.public_key);
                    println!("    {} {}", "Address:".bright_black(), key.address);
                    println!("    {} {}", "Scheme:".bright_black(), key.scheme);
                    println!("    {} {}", "Key type:".bright_black(), key.key_type);
                    if let Some(secret) = &key.secret {
                        println!("    {} {}", "Secret:".red().bold(), secret);
                    }
                }
            }
        }
        #[cfg(feature = "keyring")]
        Secp256k1Result::Keyring(r) => display_keyring_result(r),
        #[cfg(feature = "keyring")]
        Secp256k1Result::KeyringList(r) => display_keyring_list(r),
    }
}

pub fn display_ed25519_result(result: &Ed25519Result) {
    match result {
        Ed25519Result::Clear(r) => {
            println!("{} Removed {} key(s)", "✓".green().bold(), r.removed);
        }
        Ed25519Result::Generate(r) => {
            println!("{}", "✓ Generated ed25519 keypair".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "SS58 Address:".bright_blue(), r.address);
            println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
            println!("  {} {}", "Key type:".bright_blue(), r.key_type);
            if let Some(secret) = &r.secret {
                println!("  {} {}", "Secret:".red().bold(), secret);
            }
        }
        Ed25519Result::Import(r) => {
            println!("{}", "✓ Imported ed25519 key from SURI".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "SS58 Address:".bright_blue(), r.address);
            println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
            println!("  {} {}", "Key type:".bright_blue(), r.key_type);
            if let Some(secret) = &r.secret {
                println!("  {} {}", "Secret:".red().bold(), secret);
            }
        }
        Ed25519Result::Sign(r) => {
            println!("{}", "✓ Signed data".green().bold());
            println!("  {} {}", "Signature:".bright_blue(), r.signature);
        }
        Ed25519Result::Verify(_) => {
            println!("{}", "✓ Signature is valid".green().bold());
        }
        Ed25519Result::Address(r) => {
            println!("{} {}", "SS58 Address:".bright_blue(), r.address);
        }
        #[cfg(feature = "peer-id")]
        Ed25519Result::PeerId(r) => {
            println!("{} {}", "PeerId:".bright_blue(), r.peer_id);
        }
        Ed25519Result::List(r) => {
            if r.keys.is_empty() {
                println!("{}", "No keys found".yellow());
            } else {
                println!(
                    "{}",
                    format!("Found {} key(s):", r.keys.len()).green().bold()
                );
                for key in &r.keys {
                    println!("  {} {}", "•".bright_blue(), key.public_key);
                    println!("    {} {}", "SS58:".bright_black(), key.address);
                    println!("    {} {}", "Scheme:".bright_black(), key.scheme);
                    println!("    {} {}", "Key type:".bright_black(), key.key_type);
                    if let Some(secret) = &key.secret {
                        println!("    {} {}", "Secret:".red().bold(), secret);
                    }
                }
            }
        }
        #[cfg(feature = "keyring")]
        Ed25519Result::Keyring(r) => display_keyring_result(r),
        #[cfg(feature = "keyring")]
        Ed25519Result::KeyringList(r) => display_keyring_list(r),
    }
}

pub fn display_sr25519_result(result: &Sr25519Result) {
    match result {
        Sr25519Result::Clear(r) => {
            println!("{} Removed {} key(s)", "✓".green().bold(), r.removed);
        }
        Sr25519Result::Generate(r) => {
            println!("{}", "✓ Generated sr25519 keypair".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "SS58 Address:".bright_blue(), r.address);
            println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
            println!("  {} {}", "Key type:".bright_blue(), r.key_type);
            if let Some(secret) = &r.secret {
                println!("  {} {}", "Secret:".red().bold(), secret);
            }
        }
        Sr25519Result::Import(r) => {
            println!("{}", "✓ Imported sr25519 key from SURI".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", "SS58 Address:".bright_blue(), r.address);
            println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
            println!("  {} {}", "Key type:".bright_blue(), r.key_type);
            if let Some(secret) = &r.secret {
                println!("  {} {}", "Secret:".red().bold(), secret);
            }
        }
        Sr25519Result::Sign(r) => {
            println!("{}", "✓ Signed data".green().bold());
            println!("  {} {}", "Signature:".bright_blue(), r.signature);
        }
        Sr25519Result::Verify(_) => {
            println!("{}", "✓ Signature is valid".green().bold());
        }
        Sr25519Result::Address(r) => {
            println!("{} {}", "SS58 Address:".bright_blue(), r.address);
        }
        #[cfg(feature = "peer-id")]
        Sr25519Result::PeerId(r) => {
            println!("{} {}", "PeerId:".bright_blue(), r.peer_id);
        }
        #[cfg(feature = "keyring")]
        Sr25519Result::Keyring(r) => display_keyring_result(r),
        #[cfg(feature = "keyring")]
        Sr25519Result::KeyringList(r) => display_keyring_list(r),
        Sr25519Result::List(r) => {
            if r.keys.is_empty() {
                println!("{}", "No keys found".yellow());
            } else {
                println!(
                    "{}",
                    format!("Found {} key(s):", r.keys.len()).green().bold()
                );
                for key in &r.keys {
                    println!("  {} {}", "•".bright_blue(), key.public_key);
                    println!("    {} {}", "SS58:".bright_black(), key.address);
                    println!("    {} {}", "Scheme:".bright_black(), key.scheme);
                    println!("    {} {}", "Key type:".bright_black(), key.key_type);
                    if let Some(secret) = &key.secret {
                        println!("    {} {}", "Secret:".red().bold(), secret);
                    }
                }
            }
        }
    }
}
