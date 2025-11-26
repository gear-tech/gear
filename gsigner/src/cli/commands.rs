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

//! Command definitions for gsigner CLI.
//!
//! These types can be used directly with clap or integrated into other CLI applications.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

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
        help = "Password used to encrypt/decrypt the keyring (if set)"
    )]
    pub storage_password: Option<String>,
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
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum GSignerCommands {
    #[command(about = "Secp256k1 (Ethereum) operations", alias = "secp")]
    Secp256k1 {
        #[command(subcommand)]
        command: Secp256k1Commands,
    },
    #[command(about = "Ed25519 (Substrate) operations", alias = "ed")]
    Ed25519 {
        #[command(subcommand)]
        command: Ed25519Commands,
    },
    #[command(about = "Sr25519 (Substrate) operations", alias = "sr")]
    Sr25519 {
        #[command(subcommand)]
        command: Sr25519Commands,
    },
}

/// Secp256k1 subcommands
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Secp256k1Commands {
    #[command(about = "Keyring-backed operations (requires stored keys)")]
    Keyring {
        #[command(subcommand)]
        command: Secp256k1KeyringCommands,
    },
    #[command(about = "Verify a secp256k1 signature")]
    Verify {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<33>)]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(long, help = "Signature (hex)", value_parser = hex_bytes::<65>)]
        signature: String,
    },
    #[command(about = "Get Ethereum address from public key")]
    Address {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<33>)]
        public_key: String,
    },
    #[cfg(feature = "peer-id")]
    #[command(about = "Derive libp2p PeerId from public key")]
    PeerId {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<33>)]
        public_key: String,
    },
    #[command(about = "Recover public key from message and signature")]
    Recover {
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(short, long, help = "Signature (hex)", value_parser = hex_bytes::<65>)]
        signature: String,
    },
}

/// Secp256k1 keyring-backed subcommands.
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Secp256k1KeyringCommands {
    #[command(about = "Clear all keys from storage")]
    Clear {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[command(about = "Generate a new secp256k1 keypair")]
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
    #[command(about = "Sign data with a secp256k1 private key")]
    Sign {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<33>)]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short = 'p', long, help = "Prefix/salt prepended before signing")]
        prefix: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(
            short = 'c',
            long,
            help = "Contract address for EIP-191 signing (hex)",
            value_parser = hex_bytes::<20>
        )]
        contract: Option<String>,
    },
    #[command(about = "Show key info by public key or address")]
    Show {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(help = "Public key (hex) or Ethereum address (hex)")]
        key: String,
        #[arg(long, help = "Show the private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "init", about = "Initialise a JSON keyring directory")]
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
        #[arg(
            short = 'x',
            long,
            help = "Hex prefix to match (with or without 0x)",
            value_name = "HEX"
        )]
        prefix: String,
        #[arg(long, help = "Show the generated private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(
        name = "import",
        about = "Import a private key (hex) or from SURI/mnemonic into the JSON keyring"
    )]
    Import {
        #[command(flatten)]
        import: KeyringImportArgs,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "list", about = "List keys in keyring")]
    List {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
}

/// Ed25519 subcommands
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Ed25519Commands {
    #[command(about = "Keyring-backed operations (requires stored keys)")]
    Keyring {
        #[command(subcommand)]
        command: Ed25519KeyringCommands,
    },
    #[command(about = "Verify an ed25519 signature")]
    Verify {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(long, help = "Signature (hex)", value_parser = hex_bytes::<64>)]
        signature: String,
    },
    #[command(about = "Get SS58 address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(
            short = 'n',
            long,
            help = "Network prefix (numeric) or name from ss58-registry (e.g., polkadot, kusama, vara)"
        )]
        network: Option<String>,
    },
    #[cfg(feature = "peer-id")]
    #[command(about = "Derive libp2p PeerId from public key")]
    PeerId {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
    },
}

/// Ed25519 keyring-backed commands.
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Ed25519KeyringCommands {
    #[command(about = "Clear all keys from storage")]
    Clear {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[command(about = "Generate a new ed25519 keypair")]
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
    #[command(about = "Import ed25519 key from SURI/mnemonic or hex seed")]
    Import {
        #[arg(
            short = 'u',
            long,
            help = "SURI string (e.g., //Alice, //Alice//stash, mnemonic phrase)",
            conflicts_with = "seed",
            required_unless_present = "seed"
        )]
        suri: Option<String>,
        #[arg(
            short = 'k',
            long,
            help = "Seed (0x... hex, 32 bytes)",
            value_parser = hex_bytes::<32>,
            conflicts_with = "suri",
            required_unless_present = "suri"
        )]
        seed: Option<String>,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
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
    #[command(about = "Sign data with an ed25519 private key")]
    Sign {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short = 'p', long, help = "Prefix/salt prepended before signing")]
        prefix: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[command(about = "Show key info by public key")]
    Show {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
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
    #[command(name = "vanity", about = "Generate vanity SS58 address")]
    Vanity {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'x', long, help = "SS58 prefix to match")]
        prefix: String,
        #[arg(long, help = "Show the generated private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "list", about = "List keys in keyring")]
    List {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
}

/// Sr25519 subcommands
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Sr25519Commands {
    #[command(about = "Keyring-backed operations (requires stored keys)")]
    Keyring {
        #[command(subcommand)]
        command: Sr25519KeyringCommands,
    },
    #[command(about = "Verify a sr25519 signature")]
    Verify {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(
            short = 'p',
            long,
            help = "Prefix/salt that was prepended before signing"
        )]
        prefix: Option<String>,
        #[arg(short, long, help = "Signature (hex)", value_parser = hex_bytes::<64>)]
        signature: String,
        #[arg(short = 'c', long, help = "Signing context")]
        context: Option<String>,
    },
    #[command(about = "Get SS58 address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(
            short = 'n',
            long,
            help = "Network prefix (numeric) or name from ss58-registry (e.g., polkadot, kusama, vara)"
        )]
        network: Option<String>,
    },
}

/// Sr25519 keyring-backed commands.
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum Sr25519KeyringCommands {
    #[command(about = "Clear all keys from storage")]
    Clear {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[command(about = "Generate a new sr25519 keypair")]
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
    #[command(about = "Import sr25519 key from SURI/mnemonic or hex seed")]
    Import {
        #[arg(
            short = 'u',
            long,
            help = "SURI string (e.g., //Alice, //Alice//stash, mnemonic phrase)",
            conflicts_with = "seed",
            required_unless_present = "seed"
        )]
        suri: Option<String>,
        #[arg(
            short = 'k',
            long,
            help = "Seed (0x... hex, 32 bytes)",
            value_parser = hex_bytes::<32>,
            conflicts_with = "suri",
            required_unless_present = "suri"
        )]
        seed: Option<String>,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(
            long,
            help = "Show the imported private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Sign data with a sr25519 private key")]
    Sign {
        #[arg(long, help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short = 'p', long, help = "Prefix/salt prepended before signing")]
        prefix: Option<String>,
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short = 'c', long, help = "Signing context")]
        context: Option<String>,
    },
    #[command(about = "Show key info by public key")]
    Show {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(help = "Public key (hex)", value_parser = hex_bytes::<32>)]
        public_key: String,
        #[arg(long, help = "Show the private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "init", about = "Create a new keyring")]
    Init {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "vanity", about = "Generate vanity address")]
    Vanity {
        #[command(flatten)]
        storage: StorageLocationArgs,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'x', long, help = "SS58 prefix to match")]
        prefix: String,
        #[arg(long, help = "Show the generated private key", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(name = "list", about = "List keys in keyring")]
    List {
        #[command(flatten)]
        storage: StorageLocationArgs,
    },
}

/// Helper trait to inject a default storage path into commands that accept one.
pub trait WithDefaultStorage {
    fn with_default_storage(self, default: PathBuf) -> Self;
}

impl WithDefaultStorage for Secp256k1Commands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Secp256k1Commands::Keyring { command } => Secp256k1Commands::Keyring {
                command: command.with_default_storage(default),
            },
            other => other,
        }
    }
}

impl WithDefaultStorage for Secp256k1KeyringCommands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Secp256k1KeyringCommands::Clear { storage } => Secp256k1KeyringCommands::Clear {
                storage: with_opt_storage(storage, &default),
            },
            Secp256k1KeyringCommands::Generate {
                storage,
                show_secret,
            } => Secp256k1KeyringCommands::Generate {
                storage: with_opt_storage(storage, &default),
                show_secret,
            },
            Secp256k1KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage,
                contract,
            } => Secp256k1KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage: with_opt_storage(storage, &default),
                contract,
            },
            Secp256k1KeyringCommands::Show {
                storage,
                key,
                show_secret,
            } => Secp256k1KeyringCommands::Show {
                storage: with_opt_storage(storage, &default),
                key,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Secp256k1KeyringCommands::Init { .. } => self,
            #[cfg(feature = "keyring")]
            Secp256k1KeyringCommands::Create { .. } => self,
            #[cfg(feature = "keyring")]
            Secp256k1KeyringCommands::Vanity {
                storage,
                name,
                prefix,
                show_secret,
            } => Secp256k1KeyringCommands::Vanity {
                storage: with_opt_storage(storage, &default),
                name,
                prefix,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Secp256k1KeyringCommands::Import { mut import } => {
                import.storage = with_opt_storage(import.storage, &default);
                Secp256k1KeyringCommands::Import { import }
            }
            #[cfg(feature = "keyring")]
            Secp256k1KeyringCommands::List { .. } => self,
        }
    }
}

impl WithDefaultStorage for Ed25519Commands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Ed25519Commands::Keyring { command } => Ed25519Commands::Keyring {
                command: command.with_default_storage(default),
            },
            other => other,
        }
    }
}

impl WithDefaultStorage for Ed25519KeyringCommands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Ed25519KeyringCommands::Clear { storage } => Ed25519KeyringCommands::Clear {
                storage: with_opt_storage(storage, &default),
            },
            Ed25519KeyringCommands::Generate {
                storage,
                show_secret,
            } => Ed25519KeyringCommands::Generate {
                storage: with_opt_storage(storage, &default),
                show_secret,
            },
            Ed25519KeyringCommands::Import {
                suri,
                seed,
                password,
                #[cfg(feature = "keyring")]
                name,
                storage,
                show_secret,
            } => Ed25519KeyringCommands::Import {
                suri,
                seed,
                password,
                #[cfg(feature = "keyring")]
                name,
                storage: with_opt_storage(storage, &default),
                show_secret,
            },
            Ed25519KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage,
            } => Ed25519KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage: with_opt_storage(storage, &default),
            },
            Ed25519KeyringCommands::Show {
                storage,
                public_key,
                show_secret,
            } => Ed25519KeyringCommands::Show {
                storage: with_opt_storage(storage, &default),
                public_key,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Ed25519KeyringCommands::Init { .. } => self,
            #[cfg(feature = "keyring")]
            Ed25519KeyringCommands::Create { .. } => self,
            #[cfg(feature = "keyring")]
            Ed25519KeyringCommands::Vanity {
                storage,
                name,
                prefix,
                show_secret,
            } => Ed25519KeyringCommands::Vanity {
                storage: with_opt_storage(storage, &default),
                name,
                prefix,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Ed25519KeyringCommands::List { .. } => self,
        }
    }
}

impl WithDefaultStorage for Sr25519Commands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Sr25519Commands::Keyring { command } => Sr25519Commands::Keyring {
                command: command.with_default_storage(default),
            },
            other => other,
        }
    }
}

impl WithDefaultStorage for Sr25519KeyringCommands {
    fn with_default_storage(self, default: PathBuf) -> Self {
        match self {
            Sr25519KeyringCommands::Clear { storage } => Sr25519KeyringCommands::Clear {
                storage: with_opt_storage(storage, &default),
            },
            Sr25519KeyringCommands::Generate {
                storage,
                show_secret,
            } => Sr25519KeyringCommands::Generate {
                storage: with_opt_storage(storage, &default),
                show_secret,
            },
            Sr25519KeyringCommands::Import {
                suri,
                seed,
                password,
                storage,
                show_secret,
            } => Sr25519KeyringCommands::Import {
                suri,
                seed,
                password,
                storage: with_opt_storage(storage, &default),
                show_secret,
            },
            Sr25519KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage,
                context,
            } => Sr25519KeyringCommands::Sign {
                public_key,
                data,
                prefix,
                storage: with_opt_storage(storage, &default),
                context,
            },
            Sr25519KeyringCommands::Show {
                storage,
                public_key,
                show_secret,
            } => Sr25519KeyringCommands::Show {
                storage: with_opt_storage(storage, &default),
                public_key,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Sr25519KeyringCommands::Init { .. } => self,
            #[cfg(feature = "keyring")]
            Sr25519KeyringCommands::Vanity {
                storage,
                name,
                prefix,
                show_secret,
            } => Sr25519KeyringCommands::Vanity {
                storage: with_opt_storage(storage, &default),
                name,
                prefix,
                show_secret,
            },
            #[cfg(feature = "keyring")]
            Sr25519KeyringCommands::List { .. } => self,
        }
    }
}

fn with_opt_storage(mut storage: StorageLocationArgs, default: &Path) -> StorageLocationArgs {
    if storage.path.is_none() && !storage.memory {
        storage.path = Some(default.to_path_buf());
    }
    storage
}

fn hex_bytes<const N: usize>(input: &str) -> Result<String, String> {
    let trimmed = input.strip_prefix("0x").unwrap_or(input);
    if trimmed.len() != N * 2 {
        return Err(format!("expected {N}-byte hex ({} chars)", N * 2));
    }

    if trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(trimmed.to_string())
    } else {
        Err("invalid hex string".to_string())
    }
}
