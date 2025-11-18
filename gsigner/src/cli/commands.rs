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

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Root CLI structure
#[derive(Parser, Debug, Clone)]
#[command(name = "gsigner")]
#[command(about = "Universal cryptographic signer supporting secp256k1 (Ethereum), ed25519, and sr25519 (Substrate)", long_about = None)]
pub struct GSignerCli {
    #[command(subcommand)]
    pub command: GSignerCommands,
}

/// Top-level commands
#[derive(Subcommand, Debug, Clone)]
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
pub enum Secp256k1Commands {
    #[command(about = "Clear all keys from storage")]
    Clear {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
    },
    #[command(about = "Generate a new secp256k1 keypair")]
    Generate {
        #[arg(short, long, help = "Storage directory (default: memory only)")]
        storage: Option<PathBuf>,
        #[arg(
            long,
            help = "Show the generated private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Sign data with a secp256k1 private key")]
    Sign {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(short = 'c', long, help = "Contract address for EIP-191 signing (hex)")]
        contract: Option<String>,
    },
    #[command(about = "Verify a secp256k1 signature")]
    Verify {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(short, long, help = "Signature (hex)")]
        signature: String,
    },
    #[command(about = "Get Ethereum address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
    },
    #[cfg(feature = "peer-id")]
    #[command(about = "Derive libp2p PeerId from public key")]
    PeerId {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
    },
    #[command(about = "Insert a private key into storage")]
    Insert {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(help = "Private key (hex, 32 bytes)")]
        private_key: String,
        #[arg(long, help = "Show the inserted private key", default_value_t = false)]
        show_secret: bool,
    },
    #[command(about = "Show key info by public key or address")]
    Show {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(help = "Public key (hex) or Ethereum address (hex)")]
        key: String,
        #[arg(long, help = "Show the private key", default_value_t = false)]
        show_secret: bool,
    },
    #[command(about = "Recover public key from message and signature")]
    Recover {
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(short, long, help = "Signature (hex)")]
        signature: String,
    },
    #[command(about = "List all keys in storage")]
    List {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(long, help = "Show private keys (hex)", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(about = "Keyring operations")]
    Keyring {
        #[command(subcommand)]
        command: Secp256k1KeyringCommands,
    },
}

/// Ed25519 subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Ed25519Commands {
    #[command(about = "Generate a new ed25519 keypair")]
    Generate {
        #[arg(short, long, help = "Storage directory (default: memory only)")]
        storage: Option<PathBuf>,
        #[arg(
            long,
            help = "Show the generated private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Import ed25519 key from SURI (//Alice, mnemonic, etc.)")]
    Import {
        #[arg(
            short = 'u',
            long,
            help = "SURI string (e.g., //Alice, //Alice//stash, mnemonic phrase)"
        )]
        suri: String,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
        #[arg(short, long, help = "Storage directory (default: memory only)")]
        storage: Option<PathBuf>,
        #[arg(
            long,
            help = "Show the imported private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Sign data with an ed25519 private key")]
    Sign {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
    },
    #[command(about = "Verify an ed25519 signature")]
    Verify {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(short, long, help = "Signature (hex)")]
        signature: String,
    },
    #[command(about = "Get SS58 address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)")]
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
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
    },
    #[command(about = "List all keys in storage")]
    List {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(long, help = "Show private keys (hex)", default_value_t = false)]
        show_secret: bool,
    },
    #[cfg(feature = "keyring")]
    #[command(about = "Keyring operations")]
    Keyring {
        #[command(subcommand)]
        command: Ed25519KeyringCommands,
    },
}

/// Sr25519 subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Sr25519Commands {
    #[command(about = "Generate a new sr25519 keypair")]
    Generate {
        #[arg(short, long, help = "Storage directory (default: memory only)")]
        storage: Option<PathBuf>,
        #[arg(
            long,
            help = "Show the generated private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Import sr25519 key from SURI (//Alice, mnemonic, etc.)")]
    Import {
        #[arg(
            short = 'u',
            long,
            help = "SURI string (e.g., //Alice, //Alice//stash, mnemonic phrase)"
        )]
        suri: String,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
        #[arg(short, long, help = "Storage directory (default: memory only)")]
        storage: Option<PathBuf>,
        #[arg(
            long,
            help = "Show the imported private key (hex)",
            default_value_t = false
        )]
        show_secret: bool,
    },
    #[command(about = "Sign data with a sr25519 private key")]
    Sign {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data to sign (hex)")]
        data: String,
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(short = 'c', long, help = "Signing context")]
        context: Option<String>,
    },
    #[command(about = "Verify a sr25519 signature")]
    Verify {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(short, long, help = "Data that was signed (hex)")]
        data: String,
        #[arg(short, long, help = "Signature (hex)")]
        signature: String,
        #[arg(short = 'c', long, help = "Signing context")]
        context: Option<String>,
    },
    #[command(about = "Get SS58 address from public key")]
    Address {
        #[arg(short, long, help = "Public key (hex)")]
        public_key: String,
        #[arg(
            short = 'n',
            long,
            help = "Network prefix (numeric) or name from ss58-registry (e.g., polkadot, kusama, vara)"
        )]
        network: Option<String>,
    },
    #[cfg(feature = "keyring")]
    #[command(about = "Keyring operations")]
    Keyring {
        #[command(subcommand)]
        command: Sr25519KeyringCommands,
    },
    #[command(about = "List all keys in storage")]
    List {
        #[arg(short, long, help = "Storage directory")]
        storage: Option<PathBuf>,
        #[arg(long, help = "Show private keys (hex)", default_value_t = false)]
        show_secret: bool,
    },
}

#[cfg(feature = "keyring")]
/// Secp256k1 keyring subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Secp256k1KeyringCommands {
    #[command(about = "Initialise a keyring directory")]
    Create {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
    #[command(about = "Generate and store a new key")]
    Generate {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
    },
    #[command(about = "Import a private key (hex)")]
    Import {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'k', long, help = "Private key (0x... hex)")]
        private_key: String,
    },
    #[command(about = "Import a key from SURI or mnemonic")]
    ImportSuri {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'u', long, help = "SURI string or mnemonic")]
        suri: String,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
    },
    #[command(about = "List keys in keyring")]
    List {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
}

#[cfg(feature = "keyring")]
/// Ed25519 keyring subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Ed25519KeyringCommands {
    #[command(about = "Initialise a keyring directory")]
    Create {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
    #[command(about = "Generate and store a new key")]
    Generate {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
    },
    #[command(about = "Import a private key seed (hex)")]
    ImportHex {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'k', long, help = "Seed (0x... hex)")]
        seed: String,
    },
    #[command(about = "Import a key from SURI")]
    ImportSuri {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'u', long, help = "SURI string")]
        suri: String,
        #[arg(short = 'w', long, help = "Password for SURI derivation")]
        password: Option<String>,
    },
    #[command(about = "List keys in keyring")]
    List {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
}

#[cfg(feature = "keyring")]
/// Sr25519 keyring subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Sr25519KeyringCommands {
    #[command(about = "Create a new keyring")]
    Create {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
    #[command(about = "Add a key to keyring")]
    Add {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'w', long, help = "Password for encryption")]
        password: Option<String>,
    },
    #[command(about = "Generate vanity address")]
    Vanity {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
        #[arg(short, long, help = "Key name")]
        name: String,
        #[arg(short = 'x', long, help = "SS58 prefix to match")]
        prefix: String,
        #[arg(short = 'w', long, help = "Password for encryption")]
        password: Option<String>,
    },
    #[command(about = "List keys in keyring")]
    List {
        #[arg(short, long, help = "Keyring directory")]
        path: PathBuf,
    },
}
