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
}

/// Result of key import.
#[derive(Debug, Clone, Serialize)]
pub struct KeyImportResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
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
}

/// Result of keyring operations.
#[derive(Debug, Clone, Serialize)]
pub struct KeyringResult {
    pub message: String,
    pub details: Option<KeyringDetails>,
}

/// Details about keyring operation.
#[derive(Debug, Clone, Serialize)]
pub struct KeyringDetails {
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: Option<String>,
    pub keystore_name: Option<String>,
    pub private_key: Option<String>,
}

/// Result of keyring list operation.
#[derive(Debug, Clone, Serialize)]
pub struct KeyringListResult {
    pub keystores: Vec<KeystoreInfo>,
}

/// Information about a keystore.
#[derive(Debug, Clone, Serialize)]
pub struct KeystoreInfo {
    pub name: String,
    pub public_key: Option<String>,
    pub address: String,
    pub created: String,
    pub scheme: String,
    pub key_type: Option<String>,
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
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
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
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
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
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
    List(ListKeysResult),
}

/// Generic substrate-style command shared by ed25519/sr25519 flows.
enum SubstrateCommand {
    Clear {
        storage: Option<PathBuf>,
    },
    Generate {
        storage: Option<PathBuf>,
        show_secret: bool,
    },
    Import {
        suri: String,
        password: Option<String>,
        storage: Option<PathBuf>,
        show_secret: bool,
    },
    Sign {
        public_key: String,
        data: String,
        prefix: Option<String>,
        storage: Option<PathBuf>,
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
    List {
        storage: Option<PathBuf>,
        show_secret: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
enum SubstrateResult {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    #[cfg(feature = "peer-id")]
    PeerId(PeerIdResult),
    List(ListKeysResult),
}

#[cfg(feature = "keyring")]
#[derive(Debug, Clone)]
enum KeyringCommandResult {
    Keyring(KeyringResult),
    List(KeyringListResult),
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

struct SubstrateDescriptor<S: SignatureScheme> {
    formatter: SchemeFormatter<S>,
    key_type_fn: fn() -> String,
    parse_public: fn([u8; 32]) -> S::PublicKey,
    parse_signature: fn([u8; 64]) -> S::Signature,
    sign_fn: fn(&crate::Signer<S>, S::PublicKey, &[u8], Option<String>) -> Result<S::Signature>,
    verify_fn: fn(&S::PublicKey, &[u8], &S::Signature, Option<String>) -> Result<()>,
    signature_hex: fn(&S::Signature) -> String,
    import_private: fn(&str, Option<&str>) -> Result<S::PrivateKey>,
    network_scheme: crate::address::SubstrateCryptoScheme,
    #[cfg(feature = "peer-id")]
    peer_id: Option<fn(&S::PublicKey) -> Result<String>>,
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
    use crate::{
        Address,
        schemes::secp256k1::{PublicKey, Secp256k1, Secp256k1SignerExt, Signature},
    };

    let formatter = secp256k1_formatter();

    match command {
        Secp256k1Commands::Generate {
            storage,
            show_secret,
        } => {
            let result = generate_key_result::<Secp256k1>(storage, &formatter, show_secret)?;
            Ok(Secp256k1Result::Generate(result))
        }
        Secp256k1Commands::Clear { storage } => {
            let result = clear_keys_command::<Secp256k1>(storage)?;
            Ok(Secp256k1Result::Clear(result))
        }
        Secp256k1Commands::Sign {
            public_key,
            data,
            prefix,
            storage,
            contract,
        } => with_signer::<Secp256k1, _, _>(storage, |signer| {
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
        Secp256k1Commands::Insert {
            storage,
            private_key,
            show_secret,
        } => {
            let signer: crate::Signer<Secp256k1> = create_signer(storage);
            let private: <Secp256k1 as SignatureScheme>::PrivateKey = private_key.parse()?;
            let public = signer.storage_mut().add_key(private.clone())?;
            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: formatter.format_public(&public),
                address: formatter.format_address(&public.to_address()),
                scheme: formatter.scheme_name().to_string(),
                key_type: formatter.key_type(),
                secret: show_secret.then(|| hex::encode(private.seed().as_ref())),
            }))
        }
        Secp256k1Commands::Show {
            storage,
            key,
            show_secret,
        } => {
            let signer: crate::Signer<Secp256k1> = create_signer(storage);
            let key = key.strip_prefix("0x").unwrap_or(&key);
            let public = if key.len() == 66 {
                key.parse()?
            } else if key.len() == 40 {
                let mut addr = [0u8; 20];
                hex::decode_to_slice(key, &mut addr)
                    .map_err(|e| anyhow::anyhow!("Failed to parse eth address hex: {e}"))?;
                signer
                    .storage()
                    .get_key_by_address(addr.into())?
                    .ok_or_else(|| anyhow::anyhow!("Unrecognized eth address"))?
            } else {
                anyhow::bail!(
                    "Invalid key length: should be 33-byte public key or 20-byte eth address"
                );
            };
            let private = signer.storage().get_private_key(public)?;
            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: formatter.format_public(&public),
                address: formatter.format_address(&public.to_address()),
                scheme: formatter.scheme_name().to_string(),
                key_type: formatter.key_type(),
                secret: show_secret.then(|| hex::encode(private.seed().as_ref())),
            }))
        }
        #[cfg(feature = "keyring")]
        Secp256k1Commands::Keyring { command } => {
            let result = execute_secp256k1_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Secp256k1Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Secp256k1Result::KeyringList(r)),
            }
        }
        Secp256k1Commands::List {
            storage,
            show_secret,
        } => {
            let result = list_keys_result::<Secp256k1>(storage, &formatter, show_secret)?;
            Ok(Secp256k1Result::List(result))
        }
    }
}

#[cfg(not(feature = "secp256k1"))]
pub fn execute_secp256k1_command(_command: Secp256k1Commands) -> Result<Secp256k1Result> {
    anyhow::bail!("secp256k1 feature is not enabled. Rebuild with --features secp256k1");
}

#[cfg(feature = "ed25519")]
pub fn execute_ed25519_command(command: Ed25519Commands) -> Result<Ed25519Result> {
    if let Some(sub_cmd) = ed25519_to_substrate(command.clone()) {
        let res = execute_substrate_command(&ed25519_descriptor(), sub_cmd)?;
        return Ok(ed25519_from_substrate(res));
    }

    match command {
        #[cfg(feature = "keyring")]
        Ed25519Commands::Keyring { command } => {
            let result = execute_ed25519_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Ed25519Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Ed25519Result::KeyringList(r)),
            }
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
        _ => unreachable!("Handled by descriptor-driven dispatch"),
    }
}

#[cfg(not(feature = "ed25519"))]
pub fn execute_ed25519_command(_command: Ed25519Commands) -> Result<Ed25519Result> {
    anyhow::bail!("ed25519 feature is not enabled. Rebuild with --features ed25519");
}

#[cfg(feature = "sr25519")]
pub fn execute_sr25519_command(command: Sr25519Commands) -> Result<Sr25519Result> {
    if let Some(sub_cmd) = sr25519_to_substrate(command.clone()) {
        let res = execute_substrate_command(&sr25519_descriptor(), sub_cmd)?;
        return Ok(sr25519_from_substrate(res));
    }

    match command {
        #[cfg(feature = "keyring")]
        Sr25519Commands::Keyring { command } => {
            let result = execute_sr25519_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Sr25519Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Sr25519Result::KeyringList(r)),
            }
        }
        _ => unreachable!("Handled by descriptor-driven dispatch"),
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

fn decode_hex_array<const N: usize>(hex_str: &str, label: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() != N {
        anyhow::bail!("Invalid {label} length: expected {N} bytes");
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

// Helper function to create signer with optional storage.
fn create_signer<S: crate::traits::SignatureScheme>(storage: Option<PathBuf>) -> crate::Signer<S>
where
    S::PrivateKey: SeedableKey,
{
    if let Some(path) = storage {
        crate::Signer::fs(path)
    } else {
        crate::Signer::memory()
    }
}

fn with_signer<S, F, R>(storage: Option<PathBuf>, f: F) -> Result<R>
where
    S: crate::traits::SignatureScheme,
    S::PrivateKey: SeedableKey,
    F: FnOnce(crate::Signer<S>) -> Result<R>,
{
    f(create_signer::<S>(storage))
}

fn clear_keys_command<S>(storage: Option<PathBuf>) -> Result<ClearResult>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    let signer: crate::Signer<S> = create_signer(storage);
    let mut storage = signer.storage_mut();
    let len = storage.list_keys()?.len();
    storage.clear_keys()?;
    Ok(ClearResult { removed: len })
}

fn generate_key_result<S>(
    storage: Option<PathBuf>,
    formatter: &SchemeFormatter<S>,
    show_secret: bool,
) -> Result<KeyGenerationResult>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey + Clone,
{
    with_signer::<S, _, _>(storage, |signer| {
        let (private_key, public_key) = {
            let (pk, _) = S::generate_keypair();
            let public = signer.storage_mut().add_key(pk.clone())?;
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
        })
    })
}

fn list_keys_result<S>(
    storage: Option<PathBuf>,
    formatter: &SchemeFormatter<S>,
    show_secret: bool,
) -> Result<ListKeysResult>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    with_signer::<S, _, _>(storage, |signer| {
        let scheme_name = formatter.scheme_name().to_string();
        let key_type = formatter.key_type();

        let storage = signer.storage();
        let keys = storage
            .list_keys()?
            .into_iter()
            .map(|public_key| {
                let public_display = formatter.format_public(&public_key);
                let address = S::address(&public_key);
                let address_display = formatter.format_address(&address);
                let secret = if show_secret {
                    let private_key = storage.get_private_key(public_key)?;
                    Some(hex::encode(private_key.seed().as_ref()))
                } else {
                    None
                };

                Ok(KeyInfo {
                    public_key: public_display,
                    address: address_display,
                    scheme: scheme_name.clone(),
                    key_type: key_type.clone(),
                    secret,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ListKeysResult { keys })
    })
}

fn substrate_sign_command<S, Public, Sig, ParsePublic, SignerFn, SigHexFn>(
    public_key_hex: &str,
    data_hex: &str,
    prefix: &Option<String>,
    storage: Option<PathBuf>,
    parse_public: ParsePublic,
    signer_fn: SignerFn,
    sig_hex_fn: SigHexFn,
) -> Result<SignResult>
where
    S: SignatureScheme,
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

fn execute_substrate_command<S>(
    desc: &SubstrateDescriptor<S>,
    command: SubstrateCommand,
) -> Result<SubstrateResult>
where
    S: SignatureScheme<Address = crate::address::SubstrateAddress>,
    S::PrivateKey: SeedableKey + Clone,
{
    let formatter = &desc.formatter;
    match command {
        SubstrateCommand::Clear { storage } => {
            let result = clear_keys_command::<S>(storage)?;
            Ok(SubstrateResult::Clear(result))
        }
        SubstrateCommand::Generate {
            storage,
            show_secret,
        } => {
            let result = generate_key_result::<S>(storage, formatter, show_secret)?;
            Ok(SubstrateResult::Generate(result))
        }
        SubstrateCommand::Import {
            suri,
            password,
            storage,
            show_secret,
        } => with_signer::<S, _, _>(storage, |signer| {
            let private_key = (desc.import_private)(&suri, password.as_deref())?;
            let public_key = signer.import_key(private_key.clone())?;
            let public_display = formatter.format_public(&public_key);
            let address = signer.address(public_key);

            Ok(SubstrateResult::Import(KeyImportResult {
                public_key: public_display,
                address: formatter.format_address(&address),
                scheme: formatter.scheme_name().to_string(),
                key_type: (desc.key_type_fn)(),
                secret: show_secret.then(|| hex::encode(private_key.seed().as_ref())),
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
                storage,
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
        SubstrateCommand::List {
            storage,
            show_secret,
        } => {
            let result = list_keys_result::<S>(storage, formatter, show_secret)?;
            Ok(SubstrateResult::List(result))
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
        #[cfg(feature = "peer-id")]
        peer_id: None,
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
        #[cfg(feature = "peer-id")]
        peer_id: None,
    }
}

#[cfg(feature = "sr25519")]
fn sr25519_to_substrate(cmd: Sr25519Commands) -> Option<SubstrateCommand> {
    match cmd {
        Sr25519Commands::Clear { storage } => Some(SubstrateCommand::Clear { storage }),
        Sr25519Commands::Generate {
            storage,
            show_secret,
        } => Some(SubstrateCommand::Generate {
            storage,
            show_secret,
        }),
        Sr25519Commands::Import {
            suri,
            password,
            storage,
            show_secret,
        } => Some(SubstrateCommand::Import {
            suri,
            password,
            storage,
            show_secret,
        }),
        Sr25519Commands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
        } => Some(SubstrateCommand::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
        }),
        Sr25519Commands::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => Some(SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        }),
        Sr25519Commands::Address {
            public_key,
            network,
        } => Some(SubstrateCommand::Address {
            public_key,
            network,
        }),
        Sr25519Commands::List {
            storage,
            show_secret,
        } => Some(SubstrateCommand::List {
            storage,
            show_secret,
        }),
        #[cfg(feature = "keyring")]
        Sr25519Commands::Keyring { .. } => None,
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
        SubstrateResult::List(r) => Sr25519Result::List(r),
        #[cfg(feature = "peer-id")]
        SubstrateResult::PeerId(r) => Sr25519Result::PeerId(r),
    }
}

#[cfg(feature = "ed25519")]
fn ed25519_to_substrate(cmd: Ed25519Commands) -> Option<SubstrateCommand> {
    match cmd {
        Ed25519Commands::Clear { storage } => Some(SubstrateCommand::Clear { storage }),
        Ed25519Commands::Generate {
            storage,
            show_secret,
        } => Some(SubstrateCommand::Generate {
            storage,
            show_secret,
        }),
        Ed25519Commands::Import {
            suri,
            password,
            storage,
            show_secret,
        } => Some(SubstrateCommand::Import {
            suri,
            password,
            storage,
            show_secret,
        }),
        Ed25519Commands::Sign {
            public_key,
            data,
            prefix,
            storage,
        } => Some(SubstrateCommand::Sign {
            public_key,
            data,
            prefix,
            storage,
            context: None,
        }),
        Ed25519Commands::Verify {
            public_key,
            data,
            prefix,
            signature,
        } => Some(SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context: None,
        }),
        Ed25519Commands::Address {
            public_key,
            network,
        } => Some(SubstrateCommand::Address {
            public_key,
            network,
        }),
        Ed25519Commands::List {
            storage,
            show_secret,
        } => Some(SubstrateCommand::List {
            storage,
            show_secret,
        }),
        #[cfg(feature = "keyring")]
        Ed25519Commands::Keyring { .. } => None,
        #[cfg(feature = "peer-id")]
        Ed25519Commands::PeerId { .. } => None,
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
        SubstrateResult::List(r) => Ed25519Result::List(r),
        #[cfg(feature = "peer-id")]
        SubstrateResult::PeerId(r) => Ed25519Result::PeerId(r),
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
fn execute_sr25519_keyring_command(
    command: Sr25519KeyringCommands,
) -> Result<KeyringCommandResult> {
    use crate::{
        schemes::sr25519::{Keyring, PrivateKey, Sr25519},
        traits::SignatureScheme,
    };

    match command {
        Sr25519KeyringCommands::Create { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_NET);
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Created keyring at {}", path.display()),
                details: None,
            }))
        }
        Sr25519KeyringCommands::Add {
            path,
            name,
            password,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_NET);
            let mut keyring = Keyring::load(path)?;
            let private_key = PrivateKey::random();
            let passphrase = password.as_ref().map(|p| p.as_bytes());

            let keystore = keyring.add(&name, private_key.clone(), passphrase)?;
            let public_key = private_key.public_key();
            let address = public_key.to_address()?;
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = keystore.meta.name.clone();

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Added key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: hex::encode(public_key.to_bytes()),
                    address: address.as_ss58().to_string(),
                    scheme: Sr25519::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: None,
                }),
            }))
        }
        Sr25519KeyringCommands::Vanity {
            path,
            name,
            prefix,
            password,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_NET);
            let mut keyring = Keyring::load(path)?;
            let passphrase = password.as_ref().map(|p| p.as_bytes());

            let (keystore, private_key) = keyring.create_vanity(&name, &prefix, passphrase)?;
            let public_key = private_key.public_key();
            let address = public_key.to_address()?;
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = keystore.meta.name.clone();

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Generated vanity key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: hex::encode(public_key.to_bytes()),
                    address: address.as_ss58().to_string(),
                    scheme: Sr25519::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: None,
                }),
            }))
        }
        Sr25519KeyringCommands::List { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_NET);
            let keyring = Keyring::load(path)?;
            let keystores = keyring.list();

            let keystore_infos: Vec<KeystoreInfo> = keystores
                .iter()
                .map(|ks| KeystoreInfo {
                    name: ks.meta.name.clone(),
                    public_key: ks.public_key().ok().map(hex::encode),
                    address: ks.address.clone(),
                    created: ks.meta.when_created.to_string(),
                    scheme: Sr25519::scheme_name().to_string(),
                    key_type: Some(ks.meta.key_type.clone()),
                })
                .collect();

            Ok(KeyringCommandResult::List(KeyringListResult {
                keystores: keystore_infos,
            }))
        }
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn execute_secp256k1_keyring_command(
    command: Secp256k1KeyringCommands,
) -> Result<KeyringCommandResult> {
    use crate::{
        schemes::secp256k1::{Secp256k1, keyring::Keyring},
        traits::SignatureScheme,
    };

    match command {
        Secp256k1KeyringCommands::Create { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_SECP);
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Initialised keyring at {}", path.display()),
                details: None,
            }))
        }
        Secp256k1KeyringCommands::Generate {
            path,
            name,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_SECP);
            let mut keyring = Keyring::load(path)?;
            let (keystore, private_key) = keyring.create(&name)?;
            let private_hex = private_key.to_string();
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name);

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Generated key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Secp256k1::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(private_hex),
                }),
            }))
        }
        Secp256k1KeyringCommands::Import {
            path,
            name,
            private_key,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_SECP);
            let mut keyring = Keyring::load(path)?;
            let keystore = keyring.add_hex(&name, &private_key)?;
            let normalized_private = keystore.private_key()?.to_string();
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name.clone());

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Imported key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Secp256k1::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(normalized_private),
                }),
            }))
        }
        Secp256k1KeyringCommands::ImportSuri {
            path,
            name,
            suri,
            password,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_SECP);
            let mut keyring = Keyring::load(path)?;
            let (keystore, private_key) = keyring.import_suri(&name, &suri, password.as_deref())?;
            let private_hex = private_key.to_string();
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name.clone());

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Imported key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Secp256k1::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(private_hex),
                }),
            }))
        }
        Secp256k1KeyringCommands::List { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_SECP);
            let keyring = Keyring::load(path)?;
            let keystores = keyring.list();

            let keystore_infos: Vec<KeystoreInfo> = keystores
                .iter()
                .map(|ks| KeystoreInfo {
                    name: ks.name.clone(),
                    public_key: Some(ks.public_key.clone()),
                    address: ks.address.clone(),
                    created: ks.meta.when_created.to_string(),
                    scheme: Secp256k1::scheme_name().to_string(),
                    key_type: Some(ks.meta.key_type.clone()),
                })
                .collect();

            Ok(KeyringCommandResult::List(KeyringListResult {
                keystores: keystore_infos,
            }))
        }
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn execute_ed25519_keyring_command(
    command: Ed25519KeyringCommands,
) -> Result<KeyringCommandResult> {
    use crate::{
        schemes::ed25519::{Ed25519, keyring::Keyring},
        traits::SignatureScheme,
    };

    match command {
        Ed25519KeyringCommands::Create { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_ED);
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Initialised keyring at {}", path.display()),
                details: None,
            }))
        }
        Ed25519KeyringCommands::Generate {
            path,
            name,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_ED);
            let mut keyring = Keyring::load(path)?;
            let (keystore, private_key) = keyring.create(&name)?;
            let private_hex = hex::encode(private_key.to_bytes());
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name);

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Generated key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Ed25519::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::ImportHex {
            path,
            name,
            seed,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_ED);
            let mut keyring = Keyring::load(path)?;
            let keystore = keyring.add_hex(&name, &seed)?;
            let private_hex = hex::encode(keystore.private_key()?.to_bytes());
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name.clone());

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Imported key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Ed25519::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::ImportSuri {
            path,
            name,
            suri,
            password,
            show_secret,
        } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_ED);
            let mut keyring = Keyring::load(path)?;
            let (keystore, private_key) = keyring.import_suri(&name, &suri, password.as_deref())?;
            let private_hex = hex::encode(private_key.to_bytes());
            let key_type = keystore.meta.key_type.clone();
            let keystore_name = format!("{}.json", keystore.name.clone());

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Imported key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: keystore.public_key.clone(),
                    address: keystore.address.clone(),
                    scheme: Ed25519::scheme_name().to_string(),
                    key_type: Some(key_type),
                    keystore_name: Some(keystore_name),
                    private_key: show_secret.then_some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::List { path } => {
            let path = Keyring::namespaced_path(path, crate::keyring::NAMESPACE_ED);
            let keyring = Keyring::load(path)?;
            let keystores = keyring.list();

            let keystore_infos: Vec<KeystoreInfo> = keystores
                .iter()
                .map(|ks| KeystoreInfo {
                    name: ks.name.clone(),
                    public_key: Some(ks.public_key.clone()),
                    address: ks.address.clone(),
                    created: ks.meta.when_created.to_string(),
                    scheme: Ed25519::scheme_name().to_string(),
                    key_type: Some(ks.meta.key_type.clone()),
                })
                .collect();

            Ok(KeyringCommandResult::List(KeyringListResult {
                keystores: keystore_infos,
            }))
        }
    }
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
