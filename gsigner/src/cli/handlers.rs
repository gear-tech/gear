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

//! Command handlers for gsigner CLI.
//!
//! These handlers execute the commands and return structured results.
//! They don't print directly, allowing integrators to customize output formatting.

use super::commands::*;
use anyhow::Result;
use serde::Serialize;
use std::{path::PathBuf, str::FromStr};

#[cfg(all(feature = "keyring", feature = "serde"))]
trait StorageScheme: SignatureScheme + crate::keyring::KeyringScheme {}
#[cfg(all(feature = "keyring", feature = "serde"))]
impl<T> StorageScheme for T where T: SignatureScheme + crate::keyring::KeyringScheme {}

#[cfg(not(all(feature = "keyring", feature = "serde")))]
trait StorageScheme: SignatureScheme {}
#[cfg(not(all(feature = "keyring", feature = "serde")))]
impl<T> StorageScheme for T where T: SignatureScheme {}

#[cfg(any(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]
use crate::{
    substrate::pair_key_type_string,
    traits::{SeedableKey, SignatureScheme},
};

/// Result of key generation.
#[derive(Debug, Clone, Serialize)]
pub struct KeyGenerationResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Result of key import.
#[derive(Debug, Clone, Serialize)]
pub struct KeyImportResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Result of signing operation.
#[derive(Debug, Clone, Serialize)]
pub struct SignResult {
    pub signature: String,
}

/// Result of verification operation.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub valid: bool,
}

/// Result of address conversion.
#[derive(Debug, Clone, Serialize)]
pub struct AddressResult {
    pub address: String,
}

/// Result of recovering a public key from a signature.
#[derive(Debug, Clone, Serialize)]
pub struct RecoverResult {
    pub public_key: String,
    pub address: String,
}

/// Result of PeerId derivation.
#[derive(Debug, Clone, Serialize)]
pub struct PeerIdResult {
    pub peer_id: String,
}

/// Result of listing keys.
#[derive(Debug, Clone, Serialize)]
pub struct ListKeysResult {
    pub keys: Vec<KeyInfo>,
}

/// Result of clearing keys.
#[derive(Debug, Clone, Serialize)]
pub struct ClearResult {
    pub removed: usize,
}

/// Information about a key.
#[derive(Debug, Clone, Serialize)]
pub struct KeyInfo {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Generic success message result.
#[derive(Debug, Clone, Serialize)]
pub struct MessageResult {
    pub message: String,
}

/// Result of command execution.
#[derive(Debug, Clone, Serialize)]
pub enum CommandResult {
    Secp256k1(Secp256k1Result),
    Ed25519(Ed25519Result),
    Sr25519(Sr25519Result),
}

/// Result of secp256k1 command.
#[derive(Debug, Clone, Serialize)]
pub enum Secp256k1Result {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Recover(RecoverResult),
    Address(AddressResult),
    #[cfg(feature = "peer-id")]
    PeerId(PeerIdResult),
    List(ListKeysResult),
    Message(MessageResult),
}

/// Result of ed25519 command.
#[derive(Debug, Clone, Serialize)]
pub enum Ed25519Result {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    #[cfg(feature = "peer-id")]
    PeerId(PeerIdResult),
    List(ListKeysResult),
    Message(MessageResult),
}

/// Result of sr25519 command.
#[derive(Debug, Clone, Serialize)]
pub enum Sr25519Result {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    #[cfg(feature = "peer-id")]
    PeerId(PeerIdResult),
    List(ListKeysResult),
    Message(MessageResult),
}

/// Generic substrate-style command shared by ed25519/sr25519 flows.
#[cfg(any(feature = "ed25519", feature = "sr25519"))]
enum SubstrateCommand {
    Clear {
        storage: StorageLocationArgs,
    },
    Generate {
        storage: StorageLocationArgs,
        show_secret: bool,
    },
    Import {
        suri: Option<String>,
        seed: Option<String>,
        password: Option<String>,
        storage: StorageLocationArgs,
        show_secret: bool,
    },
    Sign {
        public_key: String,
        data: String,
        prefix: Option<String>,
        storage: StorageLocationArgs,
        context: Option<String>,
    },
    Verify {
        public_key: String,
        data: String,
        prefix: Option<String>,
        signature: String,
        context: Option<String>,
    },
    Address {
        public_key: String,
        network: Option<String>,
    },
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
#[derive(Debug, Clone, Serialize)]
enum SubstrateResult {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
}

struct SchemeFormatter<S: SignatureScheme> {
    scheme_name: &'static str,
    key_type_fn: fn() -> String,
    public_fmt: fn(&S::PublicKey) -> String,
    address_fmt: fn(&S::Address) -> String,
}

impl<S: SignatureScheme> SchemeFormatter<S> {
    fn scheme_name(&self) -> &'static str {
        self.scheme_name
    }

    fn key_type(&self) -> String {
        (self.key_type_fn)()
    }

    fn format_public(&self, public: &S::PublicKey) -> String {
        (self.public_fmt)(public)
    }

    fn format_address(&self, address: &S::Address) -> String {
        (self.address_fmt)(address)
    }
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
type SubstrateSignFn<S> = fn(
    &crate::Signer<S>,
    <S as SignatureScheme>::PublicKey,
    &[u8],
    Option<String>,
) -> Result<<S as SignatureScheme>::Signature>;
#[cfg(any(feature = "ed25519", feature = "sr25519"))]
type SubstrateVerifyFn<S> = fn(
    &<S as SignatureScheme>::PublicKey,
    &[u8],
    &<S as SignatureScheme>::Signature,
    Option<String>,
) -> Result<()>;

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
struct SubstrateDescriptor<S: StorageScheme> {
    formatter: SchemeFormatter<S>,
    key_type_fn: fn() -> String,
    parse_public: fn([u8; 32]) -> S::PublicKey,
    parse_signature: fn([u8; 64]) -> S::Signature,
    sign_fn: SubstrateSignFn<S>,
    verify_fn: SubstrateVerifyFn<S>,
    signature_hex: fn(&S::Signature) -> String,
    import_private: fn(&str, Option<&str>) -> Result<S::PrivateKey>,
    network_scheme: crate::address::SubstrateCryptoScheme,
}

/// Execute a gsigner command.
pub fn execute_command(command: GSignerCommands) -> Result<CommandResult> {
    match command {
        GSignerCommands::Secp256k1 { command } => {
            let result = execute_secp256k1_command(command)?;
            Ok(CommandResult::Secp256k1(result))
        }
        GSignerCommands::Ed25519 { command } => {
            let result = execute_ed25519_command(command)?;
            Ok(CommandResult::Ed25519(result))
        }
        GSignerCommands::Sr25519 { command } => {
            let result = execute_sr25519_command(command)?;
            Ok(CommandResult::Sr25519(result))
        }
    }
}

#[cfg(feature = "secp256k1")]
pub fn execute_secp256k1_command(command: Secp256k1Commands) -> Result<Secp256k1Result> {
    use crate::schemes::secp256k1::{PublicKey, Secp256k1, Signature};

    let formatter = secp256k1_formatter();

    match command {
        Secp256k1Commands::Keyring { command } => {
            execute_secp256k1_keyring_command(command, &formatter)
        }
        Secp256k1Commands::Verify {
            public_key,
            data,
            prefix,
            signature,
        } => {
            let public_key: PublicKey = public_key.parse()?;
            let message_bytes = prefixed_message(&data, &prefix)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 65];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_pre_eip155_bytes(sig_arr)
                .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;

            <Secp256k1 as SignatureScheme>::verify(&public_key, &message_bytes, &signature)?;

            Ok(Secp256k1Result::Verify(VerifyResult { valid: true }))
        }
        Secp256k1Commands::Recover {
            data,
            prefix,
            signature,
        } => {
            let message_bytes = prefixed_message(&data, &prefix)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 65];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_pre_eip155_bytes(sig_arr)
                .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;
            let public = signature.recover(&message_bytes)?;
            let address = public.to_address();

            Ok(Secp256k1Result::Recover(RecoverResult {
                public_key: formatter.format_public(&public),
                address: formatter.format_address(&address),
            }))
        }
        Secp256k1Commands::Address { public_key, .. } => {
            let public_key: PublicKey = public_key.parse()?;
            let address = public_key.to_address();

            Ok(Secp256k1Result::Address(AddressResult {
                address: formatter.format_address(&address),
            }))
        }
        #[cfg(feature = "peer-id")]
        Secp256k1Commands::PeerId { public_key } => {
            let public_key: PublicKey = public_key.parse()?;
            let peer_id = crate::peer_id::peer_id_from_secp256k1(&public_key)?;
            Ok(Secp256k1Result::PeerId(PeerIdResult {
                peer_id: peer_id.to_string(),
            }))
        }
    }
}

#[cfg(feature = "secp256k1")]
fn execute_secp256k1_keyring_command(
    command: Secp256k1KeyringCommands,
    formatter: &SchemeFormatter<crate::schemes::secp256k1::Secp256k1>,
) -> Result<Secp256k1Result> {
    use crate::{
        Address,
        schemes::secp256k1::{PublicKey, Secp256k1, Secp256k1SignerExt},
    };

    match command {
        Secp256k1KeyringCommands::Generate {
            storage,
            show_secret,
        } => {
            let result = generate_key_result::<Secp256k1>(&storage, formatter, show_secret)?;
            Ok(Secp256k1Result::Generate(result))
        }
        Secp256k1KeyringCommands::Clear { storage } => {
            let result = clear_keys_command::<Secp256k1>(&storage)?;
            Ok(Secp256k1Result::Clear(result))
        }
        Secp256k1KeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            contract,
        } => with_signer::<Secp256k1, _, _>(&storage, |signer| {
            let public_key: PublicKey = public_key.parse()?;
            let message_bytes = prefixed_message(&data, &prefix)?;

            let signature = if let Some(contract_addr) = contract {
                let contract_bytes = hex::decode(contract_addr.trim_start_matches("0x"))?;
                if contract_bytes.len() != 20 {
                    anyhow::bail!("Contract address must be 20 bytes (40 hex characters)");
                }
                let mut contract_array = [0u8; 20];
                contract_array.copy_from_slice(&contract_bytes);
                let contract_address = Address(contract_array);
                let signature =
                    signer.sign_for_contract(contract_address, public_key, &message_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            } else {
                let signature = signer.sign(public_key, &message_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            };

            Ok(Secp256k1Result::Sign(SignResult { signature }))
        }),
        Secp256k1KeyringCommands::Show {
            storage,
            key,
            show_secret,
        } => {
            let result = secp256k1_show_key(&storage, &key, show_secret, formatter)?;
            Ok(Secp256k1Result::List(result))
        }
        #[cfg(feature = "keyring")]
        Secp256k1KeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => secp256k1_keyring_vanity(storage, name, prefix, show_secret),
        #[cfg(feature = "keyring")]
        Secp256k1KeyringCommands::Init { storage } => secp256k1_keyring_init(storage),
        #[cfg(feature = "keyring")]
        Secp256k1KeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => secp256k1_keyring_generate(storage, name, show_secret),
        #[cfg(feature = "keyring")]
        Secp256k1KeyringCommands::Import { import } => secp256k1_keyring_import(import),
        #[cfg(feature = "keyring")]
        Secp256k1KeyringCommands::List { storage } => secp256k1_keyring_list(storage),
    }
}

#[cfg(not(feature = "secp256k1"))]
pub fn execute_secp256k1_command(_command: Secp256k1Commands) -> Result<Secp256k1Result> {
    anyhow::bail!("secp256k1 feature is not enabled. Rebuild with --features secp256k1");
}

#[cfg(feature = "ed25519")]
pub fn execute_ed25519_command(command: Ed25519Commands) -> Result<Ed25519Result> {
    match command {
        Ed25519Commands::Keyring { command } => execute_ed25519_keyring_command(command),
        Ed25519Commands::Verify {
            public_key,
            data,
            prefix,
            signature,
        } => {
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Verify {
                    public_key,
                    data,
                    prefix,
                    signature,
                    context: None,
                },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        Ed25519Commands::Address {
            public_key,
            network,
        } => {
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Address {
                    public_key,
                    network,
                },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        #[cfg(feature = "peer-id")]
        Ed25519Commands::PeerId { public_key } => {
            let public_key_bytes = decode_hex_array::<32>(&public_key, "public key")?;
            let public_key = crate::schemes::ed25519::PublicKey::from_bytes(public_key_bytes);
            let peer_id = crate::peer_id::peer_id_from_ed25519(&public_key)?;
            Ok(Ed25519Result::PeerId(PeerIdResult {
                peer_id: peer_id.to_string(),
            }))
        }
    }
}

#[cfg(feature = "ed25519")]
fn execute_ed25519_keyring_command(command: Ed25519KeyringCommands) -> Result<Ed25519Result> {
    match command {
        Ed25519KeyringCommands::Clear { storage } => {
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Clear { storage },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        Ed25519KeyringCommands::Generate {
            storage,
            show_secret,
        } => {
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Generate {
                    storage,
                    show_secret,
                },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        Ed25519KeyringCommands::Import {
            suri,
            seed,
            password,
            #[cfg(feature = "keyring")]
            name,
            storage,
            show_secret,
        } => {
            #[cfg(feature = "keyring")]
            if let Some(name) = name {
                return ed25519_keyring_import_named(
                    storage,
                    name,
                    seed,
                    suri,
                    password,
                    show_secret,
                );
            }
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Import {
                    suri,
                    seed,
                    password,
                    storage,
                    show_secret,
                },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        Ed25519KeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
        } => {
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Sign {
                    public_key,
                    data,
                    prefix,
                    storage,
                    context: None,
                },
            )?;
            Ok(ed25519_from_substrate(res))
        }
        Ed25519KeyringCommands::Show {
            storage,
            public_key,
            show_secret,
        } => {
            let bytes = decode_hex_array::<32>(&public_key, "public key")?;
            let public = crate::schemes::ed25519::PublicKey::from_bytes(bytes);
            let formatter = ed25519_formatter();
            let result = show_key_for_public::<crate::schemes::ed25519::Ed25519>(
                &storage,
                &formatter,
                public,
                show_secret,
            )?;
            Ok(Ed25519Result::List(result))
        }
        #[cfg(feature = "keyring")]
        Ed25519KeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => ed25519_keyring_vanity(storage, name, prefix, show_secret),
        #[cfg(feature = "keyring")]
        Ed25519KeyringCommands::Init { storage } => ed25519_keyring_init(storage),
        #[cfg(feature = "keyring")]
        Ed25519KeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => ed25519_keyring_generate(storage, name, show_secret),
        #[cfg(feature = "keyring")]
        Ed25519KeyringCommands::List { storage } => ed25519_keyring_list(storage),
    }
}

#[cfg(not(feature = "ed25519"))]
pub fn execute_ed25519_command(_command: Ed25519Commands) -> Result<Ed25519Result> {
    anyhow::bail!("ed25519 feature is not enabled. Rebuild with --features ed25519");
}

#[cfg(feature = "sr25519")]
pub fn execute_sr25519_command(command: Sr25519Commands) -> Result<Sr25519Result> {
    match command {
        Sr25519Commands::Keyring { command } => execute_sr25519_keyring_command(command),
        Sr25519Commands::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Verify {
                    public_key,
                    data,
                    prefix,
                    signature,
                    context,
                },
            )?;
            Ok(sr25519_from_substrate(res))
        }
        Sr25519Commands::Address {
            public_key,
            network,
        } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Address {
                    public_key,
                    network,
                },
            )?;
            Ok(sr25519_from_substrate(res))
        }
    }
}

#[cfg(feature = "sr25519")]
fn execute_sr25519_keyring_command(command: Sr25519KeyringCommands) -> Result<Sr25519Result> {
    match command {
        Sr25519KeyringCommands::Clear { storage } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Clear { storage },
            )?;
            Ok(sr25519_from_substrate(res))
        }
        Sr25519KeyringCommands::Generate {
            storage,
            show_secret,
        } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Generate {
                    storage,
                    show_secret,
                },
            )?;
            Ok(sr25519_from_substrate(res))
        }
        Sr25519KeyringCommands::Import {
            suri,
            seed,
            password,
            storage,
            show_secret,
        } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Import {
                    suri,
                    seed,
                    password,
                    storage,
                    show_secret,
                },
            )?;
            Ok(sr25519_from_substrate(res))
        }
        Sr25519KeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
        } => {
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Sign {
                    public_key,
                    data,
                    prefix,
                    storage,
                    context,
                },
            )?;
            Ok(sr25519_from_substrate(res))
        }
        Sr25519KeyringCommands::Show {
            storage,
            public_key,
            show_secret,
        } => {
            let bytes = decode_hex_array::<32>(&public_key, "public key")?;
            let public = crate::schemes::sr25519::PublicKey::from_bytes(bytes);
            let formatter = sr25519_formatter();
            let result = show_key_for_public::<crate::schemes::sr25519::Sr25519>(
                &storage,
                &formatter,
                public,
                show_secret,
            )?;
            Ok(Sr25519Result::List(result))
        }
        #[cfg(feature = "keyring")]
        Sr25519KeyringCommands::Init { storage } => sr25519_keyring_init(storage),
        #[cfg(feature = "keyring")]
        Sr25519KeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => sr25519_keyring_vanity(storage, name, prefix, show_secret),
        #[cfg(feature = "keyring")]
        Sr25519KeyringCommands::List { storage } => sr25519_keyring_list(storage),
    }
}

#[cfg(not(feature = "sr25519"))]
pub fn execute_sr25519_command(_command: Sr25519Commands) -> Result<Sr25519Result> {
    anyhow::bail!("sr25519 feature is not enabled. Rebuild with --features sr25519");
}

fn prefixed_message(data_hex: &str, prefix: &Option<String>) -> Result<Vec<u8>> {
    let mut message = Vec::new();

    if let Some(prefix) = prefix {
        message.extend_from_slice(prefix.as_bytes());
    }

    let data_bytes = hex::decode(data_hex)?;
    message.extend_from_slice(&data_bytes);

    Ok(message)
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn decode_hex_array<const N: usize>(hex_str: &str, label: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() != N {
        anyhow::bail!("Invalid {label} length: expected {N} bytes");
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn seed_from_hex<S>(hex_str: &str) -> Result<<S::PrivateKey as SeedableKey>::Seed>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    let bytes = decode_hex_array::<32>(hex_str, "seed")?;
    let mut seed = <S::PrivateKey as SeedableKey>::Seed::default();
    let buffer = seed.as_mut();
    if buffer.len() != bytes.len() {
        anyhow::bail!(
            "Invalid seed length: expected {} bytes, got {}",
            buffer.len(),
            bytes.len()
        );
    }
    buffer.copy_from_slice(&bytes);
    Ok(seed)
}

fn default_storage_root() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gsigner")
}

fn storage_root(path: &Option<PathBuf>) -> PathBuf {
    path.clone().unwrap_or_else(default_storage_root)
}

fn resolve_storage_location(args: &StorageLocationArgs) -> Option<PathBuf> {
    if args.memory {
        None
    } else {
        Some(storage_root(&args.path))
    }
}

#[cfg(feature = "keyring")]
enum KeyringLocation {
    Disk(PathBuf),
    Memory,
}

#[cfg(feature = "keyring")]
impl KeyringLocation {
    fn display(&self) -> String {
        match self {
            KeyringLocation::Disk(path) => path.display().to_string(),
            KeyringLocation::Memory => "<memory>".to_string(),
        }
    }
}

#[cfg(feature = "keyring")]
fn resolve_keyring_location(
    args: &StorageLocationArgs,
    namespace: &'static str,
) -> Result<KeyringLocation> {
    if args.memory {
        Ok(KeyringLocation::Memory)
    } else {
        let path = crate::keyring::resolve_namespaced_path(storage_root(&args.path), namespace);
        Ok(KeyringLocation::Disk(path))
    }
}

#[cfg(feature = "keyring")]
fn with_keyring_instance<K, LoadFn, MemFn, F, R>(
    storage: StorageLocationArgs,
    namespace: &'static str,
    load_fn: LoadFn,
    memory_fn: MemFn,
    f: F,
) -> Result<R>
where
    LoadFn: Fn(PathBuf) -> Result<K>,
    MemFn: Fn() -> K,
    F: FnOnce(KeyringLocation, &mut K, Option<String>) -> Result<R>,
{
    let password = storage.storage_password.clone();
    let location = resolve_keyring_location(&storage, namespace)?;
    let mut keyring = match &location {
        KeyringLocation::Disk(path) => load_fn(path.clone())?,
        KeyringLocation::Memory => memory_fn(),
    };
    f(location, &mut keyring, password)
}

// Helper function to create signer with optional storage.
#[cfg(all(feature = "keyring", feature = "serde"))]
fn create_signer<S>(storage: &StorageLocationArgs) -> crate::Signer<S>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    let password = storage.storage_password.clone();
    if let Some(path) = resolve_storage_location(storage) {
        crate::Signer::fs_with_password(path, password)
    } else {
        crate::Signer::memory_with_password(password)
    }
}

#[cfg(not(all(feature = "keyring", feature = "serde")))]
fn create_signer<S>(storage: &StorageLocationArgs) -> crate::Signer<S>
where
    S: crate::traits::SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    let password = storage.storage_password.clone();
    if let Some(path) = resolve_storage_location(storage) {
        crate::Signer::fs_with_password(path, password)
    } else {
        crate::Signer::memory_with_password(password)
    }
}

fn with_signer<S, F, R>(storage: &StorageLocationArgs, f: F) -> Result<R>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
    F: FnOnce(crate::Signer<S>) -> Result<R>,
{
    f(create_signer::<S>(storage))
}

fn clear_keys_command<S>(storage: &StorageLocationArgs) -> Result<ClearResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    let signer: crate::Signer<S> = create_signer(storage);
    let len = signer.list_keys()?.len();
    signer.clear_keys()?;
    Ok(ClearResult { removed: len })
}

fn generate_key_result<S>(
    storage: &StorageLocationArgs,
    formatter: &SchemeFormatter<S>,
    show_secret: bool,
) -> Result<KeyGenerationResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey + Clone,
{
    with_signer::<S, _, _>(storage, |signer| {
        let (private_key, public_key) = {
            let (pk, _) = S::generate_keypair();
            let public = signer.import_key(pk.clone())?;
            (pk, public)
        };

        let public_display = formatter.format_public(&public_key);
        let address = signer.address(public_key);
        let address_display = formatter.format_address(&address);

        Ok(KeyGenerationResult {
            public_key: public_display,
            address: address_display,
            scheme: formatter.scheme_name().to_string(),
            key_type: formatter.key_type(),
            secret: show_secret.then(|| hex::encode(private_key.seed().as_ref())),
            name: None,
        })
    })
}

fn key_info_from_public<S>(
    signer: &crate::Signer<S>,
    formatter: &SchemeFormatter<S>,
    public_key: S::PublicKey,
    show_secret: bool,
) -> Result<KeyInfo>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    if !signer.has_key(public_key.clone())? {
        anyhow::bail!("Key not found in storage");
    }

    let address = signer.address(public_key.clone());
    let secret = if show_secret {
        let private_key = signer.get_private_key(public_key.clone())?;
        Some(hex::encode(private_key.seed().as_ref()))
    } else {
        None
    };

    Ok(KeyInfo {
        public_key: formatter.format_public(&public_key),
        address: formatter.format_address(&address),
        scheme: formatter.scheme_name().to_string(),
        key_type: formatter.key_type(),
        secret,
        name: None,
    })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn show_key_for_public<S>(
    storage: &StorageLocationArgs,
    formatter: &SchemeFormatter<S>,
    public_key: S::PublicKey,
    show_secret: bool,
) -> Result<ListKeysResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    with_signer::<S, _, _>(storage, |signer| {
        let info = key_info_from_public(&signer, formatter, public_key, show_secret)?;
        Ok(ListKeysResult { keys: vec![info] })
    })
}

#[cfg(feature = "secp256k1")]
fn secp256k1_show_key(
    storage: &StorageLocationArgs,
    key: &str,
    show_secret: bool,
    formatter: &SchemeFormatter<crate::schemes::secp256k1::Secp256k1>,
) -> Result<ListKeysResult> {
    use crate::schemes::secp256k1::{Address, PublicKey, Secp256k1};

    with_signer::<Secp256k1, _, _>(storage, |signer| {
        let public_key = if let Ok(public_key) = PublicKey::from_str(key) {
            public_key
        } else {
            let address = Address::from_str(key)
                .map_err(|_| anyhow::anyhow!("Invalid public key or address '{key}'"))?;
            signer
                .get_key_by_address(address)?
                .ok_or_else(|| anyhow::anyhow!("No key found for address '{key}'"))?
        };

        let info = key_info_from_public(&signer, formatter, public_key, show_secret)?;
        Ok(ListKeysResult { keys: vec![info] })
    })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_sign_command<S, Public, Sig, ParsePublic, SignerFn, SigHexFn>(
    public_key_hex: &str,
    data_hex: &str,
    prefix: &Option<String>,
    storage: &StorageLocationArgs,
    parse_public: ParsePublic,
    signer_fn: SignerFn,
    sig_hex_fn: SigHexFn,
) -> Result<SignResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
    ParsePublic: Fn([u8; 32]) -> Public,
    SignerFn: Fn(&crate::Signer<S>, Public, &[u8]) -> Result<Sig>,
    SigHexFn: Fn(&Sig) -> String,
{
    with_signer::<S, _, _>(storage, |signer| {
        let public_key = parse_public(decode_hex_array::<32>(public_key_hex, "public key")?);
        let message_bytes = prefixed_message(data_hex, prefix)?;
        let signature = signer_fn(&signer, public_key, &message_bytes)?;

        Ok(SignResult {
            signature: sig_hex_fn(&signature),
        })
    })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_verify_command<Public, Sig, ParsePublic, ParseSig, VerifyFn>(
    public_key_hex: &str,
    data_hex: &str,
    prefix: &Option<String>,
    signature_hex: &str,
    parse_public: ParsePublic,
    parse_signature: ParseSig,
    verify_fn: VerifyFn,
) -> Result<VerifyResult>
where
    ParsePublic: Fn([u8; 32]) -> Public,
    ParseSig: Fn([u8; 64]) -> Sig,
    VerifyFn: Fn(&Public, &[u8], &Sig) -> Result<()>,
{
    let public_key = parse_public(decode_hex_array::<32>(public_key_hex, "public key")?);
    let signature = parse_signature(decode_hex_array::<64>(signature_hex, "signature")?);
    let message_bytes = prefixed_message(data_hex, prefix)?;

    verify_fn(&public_key, &message_bytes, &signature)?;

    Ok(VerifyResult { valid: true })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn execute_substrate_command<S>(
    desc: &SubstrateDescriptor<S>,
    command: SubstrateCommand,
) -> Result<SubstrateResult>
where
    S: StorageScheme,
    S: SignatureScheme<Address = crate::address::SubstrateAddress>,
    S::PrivateKey: SeedableKey + Clone,
{
    let formatter = &desc.formatter;
    match command {
        SubstrateCommand::Clear { storage } => {
            let result = clear_keys_command::<S>(&storage)?;
            Ok(SubstrateResult::Clear(result))
        }
        SubstrateCommand::Generate {
            storage,
            show_secret,
        } => {
            let result = generate_key_result::<S>(&storage, formatter, show_secret)?;
            Ok(SubstrateResult::Generate(result))
        }
        SubstrateCommand::Import {
            suri,
            seed,
            password,
            storage,
            show_secret,
        } => with_signer::<S, _, _>(&storage, |signer| {
            if password.is_some() && suri.is_none() {
                anyhow::bail!("--password can only be used together with --suri");
            }

            let private_key = if let Some(seed_hex) = seed {
                let seed_value = seed_from_hex::<S>(&seed_hex)?;
                S::PrivateKey::from_seed(seed_value)?
            } else {
                let suri = suri.expect("clap ensures either --suri or --seed is provided");
                (desc.import_private)(&suri, password.as_deref())?
            };
            let public_key = signer.import_key(private_key.clone())?;
            let public_display = formatter.format_public(&public_key);
            let address = signer.address(public_key);

            Ok(SubstrateResult::Import(KeyImportResult {
                public_key: public_display,
                address: formatter.format_address(&address),
                scheme: formatter.scheme_name().to_string(),
                key_type: (desc.key_type_fn)(),
                secret: show_secret.then(|| hex::encode(private_key.seed().as_ref())),
                name: None,
            }))
        }),
        SubstrateCommand::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
        } => {
            let result = substrate_sign_command::<S, _, _, _, _, _>(
                &public_key,
                &data,
                &prefix,
                &storage,
                desc.parse_public,
                |signer, public, message| (desc.sign_fn)(signer, public, message, context.clone()),
                |signature| (desc.signature_hex)(signature),
            )?;
            Ok(SubstrateResult::Sign(result))
        }
        SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => {
            let result = substrate_verify_command(
                &public_key,
                &data,
                &prefix,
                &signature,
                desc.parse_public,
                desc.parse_signature,
                |public, message, sig| (desc.verify_fn)(public, message, sig, context.clone()),
            )?;
            Ok(SubstrateResult::Verify(result))
        }
        SubstrateCommand::Address {
            public_key,
            network,
        } => {
            let public_key_arr = decode_hex_array::<32>(&public_key, "public key")?;
            let format = parse_ss58_format(&network)?;
            let address = crate::address::SubstrateAddress::new_with_format(
                public_key_arr,
                desc.network_scheme,
                format,
            )?;

            Ok(SubstrateResult::Address(AddressResult {
                address: formatter.format_address(&address),
            }))
        }
    }
}

#[cfg(feature = "secp256k1")]
fn secp256k1_formatter() -> SchemeFormatter<crate::schemes::secp256k1::Secp256k1> {
    SchemeFormatter {
        scheme_name: crate::schemes::secp256k1::Secp256k1::scheme_name(),
        key_type_fn: secp256k1_key_type,
        public_fmt: secp256k1_public_display,
        address_fmt: secp256k1_address_display,
    }
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_formatter<S>(
    scheme_name: &'static str,
    key_type_fn: fn() -> String,
    public_fmt: fn(&S::PublicKey) -> String,
) -> SchemeFormatter<S>
where
    S: SignatureScheme<Address = crate::address::SubstrateAddress>,
{
    SchemeFormatter {
        scheme_name,
        key_type_fn,
        public_fmt,
        address_fmt: substrate_address_display,
    }
}

#[cfg(feature = "ed25519")]
fn ed25519_formatter() -> SchemeFormatter<crate::schemes::ed25519::Ed25519> {
    substrate_formatter(
        crate::schemes::ed25519::Ed25519::scheme_name(),
        ed25519_key_type,
        ed25519_public_display,
    )
}

#[cfg(feature = "sr25519")]
fn sr25519_formatter() -> SchemeFormatter<crate::schemes::sr25519::Sr25519> {
    substrate_formatter(
        crate::schemes::sr25519::Sr25519::scheme_name(),
        sr25519_key_type,
        sr25519_public_display,
    )
}

#[cfg(feature = "ed25519")]
fn ed25519_descriptor() -> SubstrateDescriptor<crate::schemes::ed25519::Ed25519> {
    SubstrateDescriptor {
        formatter: ed25519_formatter(),
        key_type_fn: ed25519_key_type,
        parse_public: crate::schemes::ed25519::PublicKey::from_bytes,
        parse_signature: crate::schemes::ed25519::Signature::from_bytes,
        sign_fn: |signer, public, message, _context| Ok(signer.sign(public, message)?),
        verify_fn: |public, message, signature, _context| {
            Ok(
                <crate::schemes::ed25519::Ed25519 as SignatureScheme>::verify(
                    public, message, signature,
                )?,
            )
        },
        signature_hex: |signature| hex::encode(signature.to_bytes()),
        import_private: |suri, password| {
            Ok(crate::schemes::ed25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        network_scheme: crate::address::SubstrateCryptoScheme::Ed25519,
    }
}

#[cfg(feature = "sr25519")]
fn sr25519_descriptor() -> SubstrateDescriptor<crate::schemes::sr25519::Sr25519> {
    use crate::schemes::sr25519::Sr25519SignerExt;

    SubstrateDescriptor {
        formatter: sr25519_formatter(),
        key_type_fn: sr25519_key_type,
        parse_public: crate::schemes::sr25519::PublicKey::from_bytes,
        parse_signature: crate::schemes::sr25519::Signature::from_bytes,
        sign_fn: |signer, public, message, context| {
            if let Some(ctx) = context.as_deref() {
                Ok(signer.sign_with_context(public, ctx.as_bytes(), message)?)
            } else {
                Ok(signer.sign(public, message)?)
            }
        },
        verify_fn: |public, message, signature, context| {
            if let Some(ctx) = context.as_deref() {
                let signer: crate::Signer<crate::schemes::sr25519::Sr25519> =
                    crate::Signer::memory();
                Ok(signer.verify_with_context(*public, ctx.as_bytes(), message, signature)?)
            } else {
                Ok(
                    <crate::schemes::sr25519::Sr25519 as SignatureScheme>::verify(
                        public, message, signature,
                    )?,
                )
            }
        },
        signature_hex: |signature| hex::encode(signature.to_bytes()),
        import_private: |suri, password| {
            Ok(crate::schemes::sr25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        network_scheme: crate::address::SubstrateCryptoScheme::Sr25519,
    }
}

#[cfg(feature = "sr25519")]
fn sr25519_from_substrate(res: SubstrateResult) -> Sr25519Result {
    match res {
        SubstrateResult::Clear(r) => Sr25519Result::Clear(r),
        SubstrateResult::Generate(r) => Sr25519Result::Generate(r),
        SubstrateResult::Import(r) => Sr25519Result::Import(r),
        SubstrateResult::Sign(r) => Sr25519Result::Sign(r),
        SubstrateResult::Verify(r) => Sr25519Result::Verify(r),
        SubstrateResult::Address(r) => Sr25519Result::Address(r),
    }
}

#[cfg(feature = "ed25519")]
fn ed25519_from_substrate(res: SubstrateResult) -> Ed25519Result {
    match res {
        SubstrateResult::Clear(r) => Ed25519Result::Clear(r),
        SubstrateResult::Generate(r) => Ed25519Result::Generate(r),
        SubstrateResult::Import(r) => Ed25519Result::Import(r),
        SubstrateResult::Sign(r) => Ed25519Result::Sign(r),
        SubstrateResult::Verify(r) => Ed25519Result::Verify(r),
        SubstrateResult::Address(r) => Ed25519Result::Address(r),
    }
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn parse_ss58_format(network: &Option<String>) -> Result<sp_core::crypto::Ss58AddressFormat> {
    if let Some(net) = network {
        // Try numeric first.
        if let Ok(prefix) = net.parse::<u16>() {
            return Ok(sp_core::crypto::Ss58AddressFormat::custom(prefix));
        }

        // Then registry name (e.g., polkadot, kusama, vara).
        let reg = sp_core::crypto::Ss58AddressFormatRegistry::from_str(net)
            .map_err(|_| anyhow::anyhow!("Unknown network prefix '{net}'"))?;
        Ok(sp_core::crypto::Ss58AddressFormat::from(reg))
    } else {
        Ok(sp_core::crypto::Ss58AddressFormat::custom(
            crate::address::SubstrateAddress::DEFAULT_PREFIX,
        ))
    }
}

#[cfg(feature = "secp256k1")]
fn secp256k1_key_type() -> String {
    pair_key_type_string::<sp_core::ecdsa::Pair>()
}

#[cfg(feature = "secp256k1")]
fn secp256k1_public_display(key: &crate::schemes::secp256k1::PublicKey) -> String {
    key.to_hex()
}

#[cfg(feature = "secp256k1")]
fn secp256k1_address_display(address: &crate::schemes::secp256k1::Address) -> String {
    format!("0x{}", address.to_hex())
}

#[cfg(feature = "ed25519")]
fn ed25519_key_type() -> String {
    pair_key_type_string::<sp_core::ed25519::Pair>()
}

#[cfg(feature = "sr25519")]
fn sr25519_key_type() -> String {
    pair_key_type_string::<sp_core::sr25519::Pair>()
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_public_display(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(bytes.as_ref())
}

#[cfg(feature = "ed25519")]
fn ed25519_public_display(key: &crate::schemes::ed25519::PublicKey) -> String {
    substrate_public_display(key.to_bytes())
}

#[cfg(feature = "sr25519")]
fn sr25519_public_display(key: &crate::schemes::sr25519::PublicKey) -> String {
    substrate_public_display(key.to_bytes())
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_address_display(address: &crate::address::SubstrateAddress) -> String {
    address.as_ss58().to_string()
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn sr25519_keyring_init(storage: StorageLocationArgs) -> Result<Sr25519Result> {
    use crate::schemes::sr25519::Keyring;

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_NET,
        Keyring::load,
        Keyring::memory,
        |location, _, _| {
            Ok(Sr25519Result::Message(MessageResult {
                message: format!("Created keyring at {}", location.display()),
            }))
        },
    )
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn sr25519_keyring_vanity(
    storage: StorageLocationArgs,
    name: String,
    prefix: String,
    show_secret: bool,
) -> Result<Sr25519Result> {
    use crate::{
        schemes::sr25519::{Keyring, Sr25519},
        traits::SignatureScheme,
    };

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_NET,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, password| {
            let passphrase = password.as_deref().map(|p| p.as_bytes());
            let (keystore, private_key) = keyring.create_vanity(&name, &prefix, passphrase)?;
            let public_key = private_key.public_key();
            let address = public_key.to_address()?;
            let key_type = keystore.meta.key_type.clone();
            let secret = show_secret.then(|| hex::encode(private_key.to_bytes()));

            Ok(Sr25519Result::Generate(KeyGenerationResult {
                public_key: hex::encode(public_key.to_bytes()),
                address: address.as_ss58().to_string(),
                scheme: Sr25519::scheme_name().to_string(),
                key_type,
                secret,
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn sr25519_keyring_list(storage: StorageLocationArgs) -> Result<Sr25519Result> {
    use crate::schemes::sr25519::{Keyring, Sr25519};

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_NET,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, _| {
            let keys: Vec<KeyInfo> = keyring
                .list()
                .iter()
                .map(|ks| {
                    let public = ks
                        .public_key()
                        .map(hex::encode)
                        .unwrap_or_else(|_| "<unknown>".to_string());
                    KeyInfo {
                        public_key: public,
                        address: ks.address.clone(),
                        scheme: Sr25519::scheme_name().to_string(),
                        key_type: ks.meta.key_type.clone(),
                        secret: None,
                        name: Some(ks.meta.name.clone()),
                    }
                })
                .collect();

            Ok(Sr25519Result::List(ListKeysResult { keys }))
        },
    )
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_keyring_init(storage: StorageLocationArgs) -> Result<Secp256k1Result> {
    use crate::schemes::secp256k1::keyring::Keyring;

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_SECP,
        Keyring::load,
        Keyring::memory,
        |location, _, _| {
            Ok(Secp256k1Result::Message(MessageResult {
                message: format!("Initialised keyring at {}", location.display()),
            }))
        },
    )
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_keyring_generate(
    storage: StorageLocationArgs,
    name: String,
    show_secret: bool,
) -> Result<Secp256k1Result> {
    use crate::{
        schemes::secp256k1::{Secp256k1, keyring::Keyring},
        traits::SignatureScheme,
    };

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_SECP,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, password| {
            let (keystore, private_key) = keyring.create(&name, password.as_deref())?;
            let private_hex = private_key.to_string();
            let key_type = keystore.meta.key_type.clone();

            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Secp256k1::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(private_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_keyring_import(import: KeyringImportArgs) -> Result<Secp256k1Result> {
    use crate::{
        schemes::secp256k1::{Secp256k1, keyring::Keyring},
        traits::SignatureScheme,
    };

    let KeyringImportArgs {
        storage,
        name,
        private_key,
        suri,
        password: suri_password,
        show_secret,
    } = import;

    if suri_password.is_some() && suri.is_none() {
        anyhow::bail!("--password can only be used together with --suri");
    }

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_SECP,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, storage_password| {
            let storage_pass = storage_password.as_deref();
            let (keystore, private_hex) = if let Some(hex_key) = private_key {
                let keystore = keyring.add_hex(&name, &hex_key, storage_pass)?;
                let normalized = keystore
                    .private_key_with_password(storage_pass)?
                    .to_string();
                (keystore, normalized)
            } else if let Some(suri) = suri {
                let (keystore, private_key) =
                    keyring.import_suri(&name, &suri, suri_password.as_deref(), storage_pass)?;
                (keystore, private_key.to_string())
            } else {
                anyhow::bail!("either --private-key or --suri must be provided");
            };

            let key_type = keystore.meta.key_type.clone();

            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Secp256k1::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(private_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_keyring_list(storage: StorageLocationArgs) -> Result<Secp256k1Result> {
    use crate::schemes::secp256k1::{Secp256k1, keyring::Keyring};

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_SECP,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, _| {
            let keys: Vec<KeyInfo> = keyring
                .list()
                .iter()
                .map(|ks| KeyInfo {
                    public_key: ks.public_key.clone(),
                    address: ks.address.clone(),
                    scheme: Secp256k1::scheme_name().to_string(),
                    key_type: ks.meta.key_type.clone(),
                    secret: None,
                    name: Some(ks.name.clone()),
                })
                .collect();

            Ok(Secp256k1Result::List(ListKeysResult { keys }))
        },
    )
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_keyring_vanity(
    storage: StorageLocationArgs,
    name: String,
    prefix: String,
    show_secret: bool,
) -> Result<Secp256k1Result> {
    use crate::schemes::secp256k1::{PrivateKey, Secp256k1, keyring::Keyring};

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_SECP,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, storage_password| {
            let normalized = prefix.trim();
            let normalized = normalized
                .strip_prefix("0x")
                .unwrap_or(normalized)
                .to_ascii_lowercase();
            if !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
                anyhow::bail!("Prefix must be hexadecimal");
            }

            let target = normalized.clone();
            let private_key = loop {
                let candidate = PrivateKey::random();
                let address_hex = candidate.public_key().to_address().to_hex();
                if target.is_empty() || address_hex.starts_with(&target) {
                    break candidate;
                }
            };
            let private_hex = private_key.to_string();
            let keystore = keyring.add(&name, private_key.clone(), storage_password.as_deref())?;
            let key_type = keystore.meta.key_type.clone();

            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Secp256k1::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(private_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_keyring_init(storage: StorageLocationArgs) -> Result<Ed25519Result> {
    use crate::schemes::ed25519::keyring::Keyring;

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_ED,
        Keyring::load,
        Keyring::memory,
        |location, _, _| {
            Ok(Ed25519Result::Message(MessageResult {
                message: format!("Initialised keyring at {}", location.display()),
            }))
        },
    )
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_keyring_generate(
    storage: StorageLocationArgs,
    name: String,
    show_secret: bool,
) -> Result<Ed25519Result> {
    use crate::{
        schemes::ed25519::{Ed25519, keyring::Keyring},
        traits::SignatureScheme,
    };

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_ED,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, password| {
            let (keystore, private_key) = keyring.create(&name, password.as_deref())?;
            let private_hex = hex::encode(private_key.to_bytes());
            let key_type = keystore.meta.key_type.clone();

            Ok(Ed25519Result::Generate(KeyGenerationResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Ed25519::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(private_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_keyring_import_named(
    storage: StorageLocationArgs,
    name: String,
    seed: Option<String>,
    suri: Option<String>,
    password: Option<String>,
    show_secret: bool,
) -> Result<Ed25519Result> {
    use crate::{
        schemes::ed25519::{Ed25519, keyring::Keyring},
        traits::SignatureScheme,
    };

    if password.is_some() && suri.is_none() {
        anyhow::bail!("--password can only be used together with --suri");
    }

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_ED,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, storage_password| {
            let storage_pass = storage_password.as_deref();
            let (keystore, private_hex) = if let Some(seed_hex) = seed {
                let keystore = keyring.add_hex(&name, &seed_hex, storage_pass)?;
                let private =
                    hex::encode(keystore.private_key_with_password(storage_pass)?.to_bytes());
                (keystore, private)
            } else if let Some(suri) = suri {
                let (keystore, private_key) =
                    keyring.import_suri(&name, &suri, password.as_deref(), storage_pass)?;
                (keystore, hex::encode(private_key.to_bytes()))
            } else {
                anyhow::bail!("either --seed or --suri must be provided");
            };

            let key_type = keystore.meta.key_type.clone();
            Ok(Ed25519Result::Import(KeyImportResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Ed25519::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(private_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_keyring_list(storage: StorageLocationArgs) -> Result<Ed25519Result> {
    use crate::schemes::ed25519::{Ed25519, keyring::Keyring};

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_ED,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, _| {
            let keys: Vec<KeyInfo> = keyring
                .list()
                .iter()
                .map(|ks| KeyInfo {
                    public_key: ks.public_key.clone(),
                    address: ks.address.clone(),
                    scheme: Ed25519::scheme_name().to_string(),
                    key_type: ks.meta.key_type.clone(),
                    secret: None,
                    name: Some(ks.name.clone()),
                })
                .collect();

            Ok(Ed25519Result::List(ListKeysResult { keys }))
        },
    )
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_keyring_vanity(
    storage: StorageLocationArgs,
    name: String,
    prefix: String,
    show_secret: bool,
) -> Result<Ed25519Result> {
    use crate::schemes::ed25519::{Ed25519, PrivateKey, keyring::Keyring};

    with_keyring_instance(
        storage,
        crate::keyring::NAMESPACE_ED,
        Keyring::load,
        Keyring::memory,
        |_location, keyring, storage_password| {
            let private_key = loop {
                let candidate = PrivateKey::random();
                let address = candidate.public_key().to_address()?;
                if prefix.is_empty() || address.as_ss58().starts_with(&prefix) {
                    break candidate;
                }
            };
            let secret_hex = hex::encode(private_key.to_bytes());
            let keystore = keyring.add(&name, private_key.clone(), storage_password.as_deref())?;
            let key_type = keystore.meta.key_type.clone();

            Ok(Ed25519Result::Generate(KeyGenerationResult {
                public_key: keystore.public_key.clone(),
                address: keystore.address.clone(),
                scheme: Ed25519::scheme_name().to_string(),
                key_type,
                secret: show_secret.then_some(secret_hex),
                name: Some(name),
            }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::prefixed_message;

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
}
