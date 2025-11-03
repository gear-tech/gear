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
use std::path::PathBuf;

#[cfg(any(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]
use crate::{
    substrate_utils::pair_key_type_string,
    traits::{SeedableKey, SignatureScheme},
};

/// Result of key generation
#[derive(Debug, Clone)]
pub struct KeyGenerationResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
}

/// Result of key import
#[derive(Debug, Clone)]
pub struct KeyImportResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
}

/// Result of signing operation
#[derive(Debug, Clone)]
pub struct SignResult {
    pub signature: String,
}

/// Result of verification operation
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub valid: bool,
}

/// Result of address conversion
#[derive(Debug, Clone)]
pub struct AddressResult {
    pub address: String,
}

/// Result of listing keys
#[derive(Debug, Clone)]
pub struct ListKeysResult {
    pub keys: Vec<KeyInfo>,
}

/// Information about a key
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
}

/// Result of keyring operations
#[derive(Debug, Clone)]
pub struct KeyringResult {
    pub message: String,
    pub details: Option<KeyringDetails>,
}

/// Details about keyring operation
#[derive(Debug, Clone)]
pub struct KeyringDetails {
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: Option<String>,
    pub keystore_name: Option<String>,
    pub private_key: Option<String>,
}

/// Result of keyring list operation
#[derive(Debug, Clone)]
pub struct KeyringListResult {
    pub keystores: Vec<KeystoreInfo>,
}

/// Information about a keystore
#[derive(Debug, Clone)]
pub struct KeystoreInfo {
    pub name: String,
    pub public_key: Option<String>,
    pub address: String,
    pub created: String,
    pub scheme: String,
    pub key_type: Option<String>,
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

fn with_signer<S, F, R>(storage: Option<PathBuf>, f: F) -> Result<R>
where
    S: crate::traits::SignatureScheme,
    S::PrivateKey: SeedableKey,
    F: FnOnce(crate::Signer<S>) -> Result<R>,
{
    let signer = create_signer::<S>(storage);
    f(signer)
}

fn generate_key_result<S>(
    storage: Option<PathBuf>,
    formatter: &SchemeFormatter<S>,
) -> Result<KeyGenerationResult>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    with_signer::<S, _, _>(storage, |signer| {
        let public_key = signer.generate_key()?;
        let public_display = formatter.format_public(&public_key);
        let address = signer.address(public_key);
        let address_display = formatter.format_address(&address);

        Ok(KeyGenerationResult {
            public_key: public_display,
            address: address_display,
            scheme: formatter.scheme_name().to_string(),
            key_type: formatter.key_type(),
        })
    })
}

fn list_keys_result<S>(
    storage: Option<PathBuf>,
    formatter: &SchemeFormatter<S>,
) -> Result<ListKeysResult>
where
    S: SignatureScheme,
    S::PrivateKey: SeedableKey,
{
    with_signer::<S, _, _>(storage, |signer| {
        let scheme_name = formatter.scheme_name().to_string();
        let key_type = formatter.key_type();

        let keys = signer
            .list_keys()?
            .into_iter()
            .map(|public_key| {
                let public_display = formatter.format_public(&public_key);
                let address = signer.address(public_key);
                let address_display = formatter.format_address(&address);

                KeyInfo {
                    public_key: public_display,
                    address: address_display,
                    scheme: scheme_name.clone(),
                    key_type: key_type.clone(),
                }
            })
            .collect();

        Ok(ListKeysResult { keys })
    })
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

#[cfg(feature = "ed25519")]
fn ed25519_formatter() -> SchemeFormatter<crate::schemes::ed25519::Ed25519> {
    SchemeFormatter {
        scheme_name: crate::schemes::ed25519::Ed25519::scheme_name(),
        key_type_fn: ed25519_key_type,
        public_fmt: ed25519_public_display,
        address_fmt: substrate_address_display,
    }
}

#[cfg(feature = "sr25519")]
fn sr25519_formatter() -> SchemeFormatter<crate::schemes::sr25519::Sr25519> {
    SchemeFormatter {
        scheme_name: crate::schemes::sr25519::Sr25519::scheme_name(),
        key_type_fn: sr25519_key_type,
        public_fmt: sr25519_public_display,
        address_fmt: substrate_address_display,
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

fn ed25519_public_display(key: &crate::schemes::ed25519::PublicKey) -> String {
    hex::encode(key.to_bytes())
}

#[cfg(feature = "sr25519")]
fn sr25519_public_display(key: &crate::schemes::sr25519::PublicKey) -> String {
    hex::encode(key.to_bytes())
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
fn substrate_address_display(address: &crate::address::SubstrateAddress) -> String {
    address.as_ss58().to_string()
}

/// Execute a gsigner command
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

/// Result of command execution
#[derive(Debug, Clone)]
pub enum CommandResult {
    Secp256k1(Secp256k1Result),
    Ed25519(Ed25519Result),
    Sr25519(Sr25519Result),
}

/// Result of secp256k1 command
#[derive(Debug, Clone)]
pub enum Secp256k1Result {
    Generate(KeyGenerationResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    List(ListKeysResult),
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
}

/// Result of ed25519 command
#[derive(Debug, Clone)]
pub enum Ed25519Result {
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    List(ListKeysResult),
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
}

/// Result of sr25519 command
#[derive(Debug, Clone)]
pub enum Sr25519Result {
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    #[cfg(feature = "keyring")]
    Keyring(KeyringResult),
    #[cfg(feature = "keyring")]
    KeyringList(KeyringListResult),
    List(ListKeysResult),
}

#[cfg(feature = "secp256k1")]
pub fn execute_secp256k1_command(command: Secp256k1Commands) -> Result<Secp256k1Result> {
    use crate::{
        Address,
        schemes::secp256k1::{PublicKey, Secp256k1, Secp256k1SignerExt, Signature},
    };

    let formatter = secp256k1_formatter();

    match command {
        Secp256k1Commands::Generate { storage } => {
            let result = generate_key_result::<Secp256k1>(storage, &formatter)?;
            Ok(Secp256k1Result::Generate(result))
        }
        Secp256k1Commands::Sign {
            public_key,
            data,
            storage,
            contract,
        } => with_signer::<Secp256k1, _, _>(storage, |signer| {
            let public_key: PublicKey = public_key.parse()?;
            let data_bytes = hex::decode(&data)?;

            let signature = if let Some(contract_addr) = contract {
                let contract_bytes = hex::decode(contract_addr.trim_start_matches("0x"))?;
                let mut contract_array = [0u8; 20];
                contract_array.copy_from_slice(&contract_bytes);
                let contract_address = Address(contract_array);
                let signature =
                    signer.sign_for_contract(contract_address, public_key, &data_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            } else {
                let signature = signer.sign(public_key, &data_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            };

            Ok(Secp256k1Result::Sign(SignResult { signature }))
        }),
        Secp256k1Commands::Verify {
            public_key,
            data,
            signature,
        } => {
            let public_key: PublicKey = public_key.parse()?;
            let data_bytes = hex::decode(&data)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 65];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_pre_eip155_bytes(sig_arr)
                .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;

            <Secp256k1 as SignatureScheme>::verify(&public_key, &data_bytes, &signature)?;

            Ok(Secp256k1Result::Verify(VerifyResult { valid: true }))
        }
        Secp256k1Commands::Address { public_key } => {
            let public_key: PublicKey = public_key.parse()?;
            let address = public_key.to_address();

            Ok(Secp256k1Result::Address(AddressResult {
                address: formatter.format_address(&address),
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
        Secp256k1Commands::List { storage } => {
            let result = list_keys_result::<Secp256k1>(storage, &formatter)?;
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
    use crate::schemes::ed25519::{Ed25519, PrivateKey, PublicKey, Signature};

    let formatter = ed25519_formatter();

    match command {
        Ed25519Commands::Generate { storage } => {
            let result = generate_key_result::<Ed25519>(storage, &formatter)?;
            Ok(Ed25519Result::Generate(result))
        }
        Ed25519Commands::Import {
            suri,
            password,
            storage,
        } => with_signer::<Ed25519, _, _>(storage, |signer| {
            let private_key = PrivateKey::from_suri(&suri, password.as_deref())?;
            let public_key = signer.import_key(private_key)?;
            let address = signer.address(public_key);

            Ok(Ed25519Result::Import(KeyImportResult {
                public_key: formatter.format_public(&public_key),
                address: formatter.format_address(&address),
                scheme: formatter.scheme_name().to_string(),
                key_type: formatter.key_type(),
            }))
        }),
        Ed25519Commands::Sign {
            public_key,
            data,
            storage,
        } => with_signer::<Ed25519, _, _>(storage, |signer| {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;

            let signature = signer.sign(public_key, &data_bytes)?;

            Ok(Ed25519Result::Sign(SignResult {
                signature: hex::encode(signature.to_bytes()),
            }))
        }),
        Ed25519Commands::Verify {
            public_key,
            data,
            signature,
        } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 64];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_bytes(sig_arr);

            <Ed25519 as SignatureScheme>::verify(&public_key, &data_bytes, &signature)?;

            Ok(Ed25519Result::Verify(VerifyResult { valid: true }))
        }
        Ed25519Commands::Address { public_key } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let address = crate::address::SubstrateAddress::new(
                public_key_arr,
                crate::address::SubstrateCryptoScheme::Ed25519,
            )?;

            Ok(Ed25519Result::Address(AddressResult {
                address: formatter.format_address(&address),
            }))
        }
        #[cfg(feature = "keyring")]
        Ed25519Commands::Keyring { command } => {
            let result = execute_ed25519_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Ed25519Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Ed25519Result::KeyringList(r)),
            }
        }
        Ed25519Commands::List { storage } => {
            let result = list_keys_result::<Ed25519>(storage, &formatter)?;
            Ok(Ed25519Result::List(result))
        }
    }
}

#[cfg(not(feature = "ed25519"))]
pub fn execute_ed25519_command(_command: Ed25519Commands) -> Result<Ed25519Result> {
    anyhow::bail!("ed25519 feature is not enabled. Rebuild with --features ed25519");
}

#[cfg(feature = "sr25519")]
pub fn execute_sr25519_command(command: Sr25519Commands) -> Result<Sr25519Result> {
    use crate::{
        Signer,
        schemes::sr25519::{PublicKey, Signature, Sr25519, Sr25519SignerExt},
    };

    let formatter = sr25519_formatter();

    match command {
        Sr25519Commands::Generate { storage } => {
            let result = generate_key_result::<Sr25519>(storage, &formatter)?;
            Ok(Sr25519Result::Generate(result))
        }
        Sr25519Commands::Import {
            suri,
            password,
            storage,
        } => with_signer::<Sr25519, _, _>(storage, |signer| {
            use crate::schemes::sr25519::PrivateKey;

            let private_key = PrivateKey::from_suri(&suri, password.as_deref())?;
            let public_key = signer.import_key(private_key)?;
            let address = signer.address(public_key);

            Ok(Sr25519Result::Import(KeyImportResult {
                public_key: formatter.format_public(&public_key),
                address: formatter.format_address(&address),
                scheme: formatter.scheme_name().to_string(),
                key_type: formatter.key_type(),
            }))
        }),
        Sr25519Commands::Sign {
            public_key,
            data,
            storage,
            context,
        } => with_signer::<Sr25519, _, _>(storage, |signer| {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;

            let signature = if let Some(ctx) = context {
                signer.sign_with_context(public_key, ctx.as_bytes(), &data_bytes)?
            } else {
                signer.sign(public_key, &data_bytes)?
            };

            Ok(Sr25519Result::Sign(SignResult {
                signature: hex::encode(signature.to_bytes()),
            }))
        }),
        Sr25519Commands::Verify {
            public_key,
            data,
            signature,
            context,
        } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 64];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_bytes(sig_arr);

            if let Some(ctx) = context {
                let signer: Signer<Sr25519> = Signer::memory();
                signer.verify_with_context(public_key, ctx.as_bytes(), &data_bytes, &signature)?;
            } else {
                <Sr25519 as SignatureScheme>::verify(&public_key, &data_bytes, &signature)?;
            }

            Ok(Sr25519Result::Verify(VerifyResult { valid: true }))
        }
        Sr25519Commands::Address { public_key } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let address = crate::address::SubstrateAddress::new(
                public_key_arr,
                crate::address::SubstrateCryptoScheme::Sr25519,
            )?;

            Ok(Sr25519Result::Address(AddressResult {
                address: formatter.format_address(&address),
            }))
        }
        #[cfg(feature = "keyring")]
        Sr25519Commands::Keyring { command } => {
            let result = execute_sr25519_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Sr25519Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Sr25519Result::KeyringList(r)),
            }
        }
        Sr25519Commands::List { storage } => {
            let result = list_keys_result::<Sr25519>(storage, &formatter)?;
            Ok(Sr25519Result::List(result))
        }
    }
}

#[cfg(not(feature = "sr25519"))]
pub fn execute_sr25519_command(_command: Sr25519Commands) -> Result<Sr25519Result> {
    anyhow::bail!("sr25519 feature is not enabled. Rebuild with --features sr25519");
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
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Initialised keyring at {}", path.display()),
                details: None,
            }))
        }
        Secp256k1KeyringCommands::Generate { path, name } => {
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
                    private_key: Some(private_hex),
                }),
            }))
        }
        Secp256k1KeyringCommands::Import {
            path,
            name,
            private_key,
        } => {
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
                    private_key: Some(normalized_private),
                }),
            }))
        }
        Secp256k1KeyringCommands::ImportSuri {
            path,
            name,
            suri,
            password,
        } => {
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
                    private_key: Some(private_hex),
                }),
            }))
        }
        Secp256k1KeyringCommands::List { path } => {
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
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Initialised keyring at {}", path.display()),
                details: None,
            }))
        }
        Ed25519KeyringCommands::Generate { path, name } => {
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
                    private_key: Some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::ImportHex { path, name, seed } => {
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
                    private_key: Some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::ImportSuri {
            path,
            name,
            suri,
            password,
        } => {
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
                    private_key: Some(private_hex),
                }),
            }))
        }
        Ed25519KeyringCommands::List { path } => {
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

// Helper function to create signer with optional storage
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
