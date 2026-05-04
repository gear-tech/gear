// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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

use crate::cli::{commands::OutputFormat, scheme::*};
use colored::Colorize;
use serde_json;

/// Display a command result with the desired format.
pub fn display_result_with_format(result: &CommandResult, format: OutputFormat) {
    match format {
        OutputFormat::Human => display_result(result),
        OutputFormat::Plain => display_plain(result),
        OutputFormat::Json => display_json(result),
    }
}

/// Default human-friendly output.
pub fn display_result(result: &CommandResult) {
    display_scheme(result.scheme, &result.result);
}

fn display_plain(result: &CommandResult) {
    match serde_json::to_string_pretty(result) {
        Ok(json) => println!("{json}"),
        Err(err) => eprintln!("Failed to render result: {err}"),
    }
}

fn display_json(result: &CommandResult) {
    match serde_json::to_string(result) {
        Ok(json) => println!("{json}"),
        Err(err) => eprintln!("Failed to render result: {err}"),
    }
}

fn display_clear(r: &ClearResult) {
    println!("{} Removed {} key(s)", "✓".green().bold(), r.removed);
}

fn display_generate(r: &KeyGenerationResult, address_caption: &str) {
    println!("{}", "✓ Generated keypair".green().bold());
    if let Some(name) = &r.name {
        println!("  {} {}", "Name:".bright_blue(), name);
    }
    println!("  {} {}", "Public key:".bright_blue(), r.public_key);
    println!("  {} {}", address_caption.bright_blue(), r.address);
    println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
    if let Some(secret) = &r.secret {
        println!("  {} {}", "Secret:".red().bold(), secret);
    }
}

fn display_import(r: &KeyImportResult, address_caption: &str, scheme_label: &str) {
    println!(
        "{}",
        format!("✓ Imported {scheme_label} key").green().bold()
    );
    if let Some(name) = &r.name {
        println!("  {} {}", "Name:".bright_blue(), name);
    }
    println!("  {} {}", "Public key:".bright_blue(), r.public_key);
    println!("  {} {}", address_caption.bright_blue(), r.address);
    println!("  {} {}", "Scheme:".bright_blue(), r.scheme);
    if let Some(secret) = &r.secret {
        println!("  {} {}", "Secret:".red().bold(), secret);
    }
}

fn display_sign(r: &SignResult) {
    println!("{}", "✓ Signed data".green().bold());
    println!("  {} {}", "Signature:".bright_blue(), r.signature);
}

fn display_verify() {
    println!("{}", "✓ Signature is valid".green().bold());
}

fn display_address(label: &str, address: &str) {
    println!("{} {}", label.bright_blue(), address);
}

#[cfg(feature = "peer-id")]
fn display_peer_id(peer_id: &str) {
    println!("{} {}", "PeerId:".bright_blue(), peer_id);
}

fn display_list(result: &ListKeysResult, address_caption: &str) {
    if result.keys.is_empty() {
        println!("{}", "No keys found".yellow());
    } else {
        println!(
            "{}",
            format!("Found {} key(s):", result.keys.len())
                .green()
                .bold()
        );
        for key in &result.keys {
            if let Some(name) = &key.name {
                println!("  {} {}", "•".bright_blue(), name.bright_white().bold());
                println!("    {} {}", "Public key:".bright_black(), key.public_key);
            } else {
                println!("  {} {}", "•".bright_blue(), key.public_key.clone());
            }
            println!("    {} {}", address_caption.bright_black(), key.address);
            println!("    {} {}", "Scheme:".bright_black(), key.scheme);
            if let Some(secret) = &key.secret {
                println!("    {} {}", "Secret:".red().bold(), secret);
            }
        }
    }
}

fn display_message(result: &MessageResult) {
    println!("{}", format!("✓ {}", result.message).green().bold());
}

fn display_scheme(scheme: Scheme, result: &SchemeResult) {
    match scheme {
        Scheme::Secp256k1 => display_scheme_result(result, "Address:", "secp256k1"),
        Scheme::Ed25519 => display_scheme_result(result, "SS58 Address:", "ed25519"),
        Scheme::Sr25519 => display_scheme_result(result, "SS58 Address:", "sr25519"),
    }
}

fn display_scheme_result(result: &SchemeResult, address_caption: &str, scheme_label: &str) {
    match result {
        SchemeResult::Clear(r) => display_clear(r),
        SchemeResult::Generate(r) => display_generate(r, address_caption),
        SchemeResult::Import(r) => display_import(r, address_caption, scheme_label),
        SchemeResult::Sign(r) => display_sign(r),
        SchemeResult::Verify(_) => display_verify(),
        SchemeResult::Recover(r) => {
            println!("{}", "✓ Recovered public key".green().bold());
            println!("  {} {}", "Public key:".bright_blue(), r.public_key);
            println!("  {} {}", address_caption.bright_blue(), r.address);
        }
        SchemeResult::Address(r) => display_address(address_caption, &r.address),
        SchemeResult::PeerId(r) => {
            #[cfg(feature = "peer-id")]
            {
                display_peer_id(&r.peer_id);
            }
            #[cfg(not(feature = "peer-id"))]
            {
                let _ = r;
            }
        }
        SchemeResult::List(r) => display_list(r, address_caption),
        SchemeResult::Message(r) => display_message(r),
    }
}
