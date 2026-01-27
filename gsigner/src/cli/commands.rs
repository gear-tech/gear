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

//! Command definitions for gsigner CLI.
//!
//! These types can be used directly with clap or integrated into other CLI applications.

use crate::cli::util::hex_bytes;
use clap::{Args, Parser, Subcommand, ValueEnum};
use secrecy::SecretString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Human,
    Plain,
    Json,
}

/// Shared arguments controlling where keys are stored.
#[derive(Debug, Clone, Args)]
pub struct StorageLocationArgs {
    #[arg(
        short = 's',
        long = "path",
        alias = "storage",
        value_name = "PATH",
        help = "Key storage path (defaults to the gsigner data directory)"
    )]
    pub path: Option<PathBuf>,
    #[arg(
        long,
        help = "Use in-memory storage (do not persist keys)",
        default_value_t = false,
        conflicts_with = "path"
    )]
    pub memory: bool,
    #[arg(
        long = "storage-password",
        value_name = "PASSWORD",
        help = "Password used to encrypt/decrypt the keyring (if set)",
        value_parser = secret_string_parser
    )]
    pub storage_password: Option<SecretString>,
}

/// Storage location options without a password.
#[derive(Debug, Clone, Args)]
pub struct StorageLocationPathArgs {
    #[arg(
        short = 's',
        long = "path",
        alias = "storage",
        value_name = "PATH",
        help = "Key storage path (defaults to the gsigner data directory)"
    )]
    pub path: Option<PathBuf>,
    #[arg(
        long,
        help = "Use in-memory storage (do not persist keys)",
        default_value_t = false,
        conflicts_with = "path"
    )]
    pub memory: bool,
}

impl StorageLocationPathArgs {
    pub fn into_storage_args(self) -> StorageLocationArgs {
        StorageLocationArgs {
            path: self.path,
            memory: self.memory,
            storage_password: None,
        }
    }
}

fn secret_string_parser(s: &str) -> Result<SecretString, String> {
    Ok(SecretString::new(s.to_owned()))
}

#[cfg(feature = "keyring")]
#[derive(Debug, Clone, Args)]
pub struct KeyringImportArgs {
    #[command(flatten)]
    pub storage: StorageLocationArgs,
    #[arg(short, long, help = "Key name")]
    pub name: String,
    #[arg(
        short = 'k',
        long,
        help = "Private key (0x... hex, 32 bytes)",
        value_parser = hex_bytes::<32>,
        conflicts_with = "suri",
        required_unless_present = "suri"
    )]
    pub private_key: Option<String>,
    #[arg(
        short = 'u',
        long,
        help = "SURI string or mnemonic",
        conflicts_with = "private_key",
        required_unless_present = "private_key"
    )]
    pub suri: Option<String>,
    #[arg(short = 'w', long, help = "Password for SURI derivation")]
    pub password: Option<String>,
    #[arg(long, help = "Show the imported private key", default_value_t = false)]
    pub show_secret: bool,
}

/// Root CLI structure
#[derive(Parser, Debug, Clone)]
#[command(name = "gsigner")]
#[command(about = "Universal cryptographic signer supporting secp256k1 (Ethereum), ed25519, and sr25519 (Substrate)", long_about = None)]
pub struct GSignerCli {
    #[arg(
        long,
        value_enum,
        default_value_t = OutputFormat::Human,
        help = "Output format (human, plain, json)"
    )]
    pub format: OutputFormat,
    #[command(subcommand)]
    pub command: GSignerCommands,
}

/// Top-level commands
pub type GSignerCommands = SchemeCommands;

impl GSignerCommands {
    pub fn into_scheme_and_subcommand(self) -> (crate::cli::scheme::Scheme, SchemeSubcommand) {
        match self {
            SchemeCommands::Secp256k1 { command } => {
                (crate::cli::scheme::Scheme::Secp256k1, command)
            }
            SchemeCommands::Ed25519 { command } => (crate::cli::scheme::Scheme::Ed25519, command),
            SchemeCommands::Sr25519 { command } => (crate::cli::scheme::Scheme::Sr25519, command),
        }
    }
}
/// Unified scheme commands
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum SchemeCommands {
    #[command(about = "Secp256k1 (Ethereum) operations", alias = "secp")]
    Secp256k1 {
        #[command(subcommand)]
        command: SchemeSubcommand,
    },
    #[command(about = "Ed25519 (Substrate) operations", alias = "ed")]
    Ed25519 {
        #[command(subcommand)]
        command: SchemeSubcommand,
    },
    #[command(about = "Sr25519 (Substrate) operations", alias = "sr")]
    Sr25519 {
        #[command(subcommand)]
        command: SchemeSubcommand,
    },
}

/// Common subcommands across schemes
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum SchemeSubcommand {
    #[cfg(feature = "keyring")]
    #[command(about = "Keyring-backed operations (requires stored keys)")]
    Keyring {
        #[command(subcommand)]
        command: SchemeKeyringCommands,
    },
    #[command(about = "Verify signature")]
    Verify {
        #[arg(long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(long, help = "Signature (hex)")]
        signature: String,
        #[arg(short = 'c', long, help = "Signing context (sr25519)")]
        context: Option<String>,
    },
    #[command(about = "Get address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(
            short = 'n',
            long,
            help = "Network prefix (numeric) or name from ss58-registry"
        )]
        network: Option<String>,
    },
    #[command(about = "Recover public key from message and signature", alias = "rec")]
    Recover {
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(short, long, help = "Signature (hex)")]
        signature: String,
    },
    #[cfg(feature = "peer-id")]
    #[command(about = "Derive PeerId from public key")]
    PeerId {
        #[arg(long, help = "Public key (hex)")]
        public_key: String,
    },
}

/// Unified keyring-backed commands
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum SchemeKeyringCommands {
    #[command(about = "Clear all keys from storage")]
    Clear {
        #[command(flatten)]
        storage: StorageLocationPathArgs,
    },
    #[command(about = "Generate and store a new keypair")]
    Generate {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(
            long,
            help = "Show the generated private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Import key (hex seed or SURI)")]
    Import {
        #[arg(
            short = 'u',
            long,
            help = "SURI string or mnemonic",
            conflicts_with = "seed"
        )]
        suri: Option<String>,
        #[arg(short = 'k', long, help = "Seed (0x... hex, 32 bytes)", value_parser = hex_bytes::<32>, conflicts_with = "suri")]
        seed: Option<String>,
        #[arg(short = 'p', long = "private-key", help = "Private key (0x... hex, 32 bytes)", value_parser = hex_bytes::<32>, conflicts_with_all = ["seed", "suri"])]
        private_key: Option<String>,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        suri_password: Option<String>,
        #[cfg(feature = "keyring")]
        #[arg(short, long, help = "Key name for JSON keyring entry")]
        name: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(
            long,
            help = "Show the imported private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Sign data with a private key")]
    Sign {
        #[arg(long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short = 'p', long, help = "Prefix/salt prepended before signing")]
        prefix: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short = 'c', long, help = "Signing context (sr25519)")]
        context: Option<String>,
        #[arg(
            short = 'a',
            long = "contract",
            help = "Contract address for secp256k1 (hex)"
        )]
        contract: Option<String>,
    },
    #[command(about = "Show key info by public key or address")]
    Show {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(help = "Public key (hex) or address (hex/ss58)")]
        key: String,
        #[arg(long, help = "Show the private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "init", about = "Initialise a keyring directory")]
    Init {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "create", about = "Generate and store a named key")]
    Create {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(long, help = "Show the generated private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "vanity", about = "Generate vanity address")]
    Vanity {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'x', long, help = "Prefix to match (hex or ss58 prefix)")]
        prefix: String,
        #[arg(long, help = "Show the generated private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "list", about = "List keys in keyring")]
    List {
        #[command(flatten)]
        storage: StorageLocationPathArgs,
    },
}
