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

//! Command handlers for gsigner CLI.
//!
//! These handlers execute the commands and return structured results.
//! They don't print directly, allowing integrators to customize output formatting.

use super::{
    commands::*,
    keyring_ops::*,
    scheme::*,
    secp::*,
    storage::*,
    util::{prefixed_message, strip_0x, validate_hex_len},
};
#[cfg(feature = "secp256k1")]
use alloy_primitives::utils::EIP191_PREFIX;
use anyhow::Result;
use secrecy::ExposeSecret;
use std::{path::PathBuf, str::FromStr};

#[cfg(any(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]
use crate::traits::SignatureScheme;

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
use super::substrate::*;
#[cfg(any(feature = "ed25519", feature = "sr25519"))]
use crate::cli::util::decode_hex_array;

/// Execute a gsigner command.
pub fn execute_command(command: GSignerCommands) -> Result<CommandResult> {
    let (scheme, command) = command.into_scheme_and_subcommand();
    let handler = scheme_handlers()
        .into_iter()
        .find(|entry| entry.scheme == scheme)
        .ok_or_else(|| anyhow::anyhow!("handler for scheme {:?} is not available", scheme))?;

    let result = (handler.handler)(command)?;
    Ok(CommandResult { scheme, result })
}

struct SchemeHandlerEntry {
    scheme: Scheme,
    handler: fn(SchemeSubcommand) -> Result<SchemeResult>,
}

fn dispatch_subcommand(
    descriptor: &SchemeDescriptor<SchemeKeyringCommands>,
    command: SchemeSubcommand,
) -> Result<SchemeResult> {
    match command {
        #[cfg(feature = "keyring")]
        SchemeSubcommand::Keyring { command } => {
            execute_scheme_command(descriptor, SchemeCommand::Keyring { command })
        }
        SchemeSubcommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Verify {
                public_key,
                data,
                prefix,
                signature,
                context,
            },
        ),
        SchemeSubcommand::Address {
            public_key,
            network,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Address {
                public_key,
                network,
            },
        ),
        SchemeSubcommand::Recover {
            data,
            prefix,
            signature,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Recover {
                data,
                prefix,
                signature,
            },
        ),
        #[cfg(feature = "peer-id")]
        SchemeSubcommand::PeerId { public_key } => {
            execute_scheme_command(descriptor, SchemeCommand::PeerId { public_key })
        }
    }
}

#[allow(clippy::vec_init_then_push)]
fn scheme_handlers() -> Vec<SchemeHandlerEntry> {
    let mut entries = Vec::new();
    #[cfg(feature = "secp256k1")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Secp256k1,
        handler: |cmd| dispatch_subcommand(&secp256k1_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "secp256k1"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Secp256k1,
        handler: |_cmd| {
            anyhow::bail!("secp256k1 feature is not enabled. Rebuild with --features secp256k1")
        },
    });

    #[cfg(feature = "ed25519")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Ed25519,
        handler: |cmd| dispatch_subcommand(&ed25519_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "ed25519"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Ed25519,
        handler: |_cmd| {
            anyhow::bail!("ed25519 feature is not enabled. Rebuild with --features ed25519")
        },
    });

    #[cfg(feature = "sr25519")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Sr25519,
        handler: |cmd| dispatch_subcommand(&sr25519_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "sr25519"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Sr25519,
        handler: |_cmd| {
            anyhow::bail!("sr25519 feature is not enabled. Rebuild with --features sr25519")
        },
    });

    entries
}

#[cfg(feature = "secp256k1")]
fn secp256k1_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        name: crate::schemes::secp256k1::Secp256k1::scheme_name(),
        #[cfg(feature = "keyring")]
        handle_keyring: secp256k1_handle_keyring,
        verify: |public_key, data, prefix, signature, _| {
            let effective_prefix = prefix.or_else(|| Some(EIP191_PREFIX.to_string()));
            verify(public_key, data, effective_prefix, signature)
        },
        address: |public_key, _network| address(public_key),
        recover: Some(|data, prefix, signature| {
            let effective_prefix = prefix.or_else(|| Some(EIP191_PREFIX.to_string()));
            recover(data, effective_prefix, signature)
        }),
        #[cfg(feature = "peer-id")]
        peer_id: Some(secp256k1_peer_id),
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    let formatter = secp256k1_formatter();
    execute_secp256k1_keyring_command(command, &formatter)
}

#[cfg(all(feature = "secp256k1", feature = "peer-id"))]
fn secp256k1_peer_id(public_key: String) -> Result<SchemeResult> {
    use crate::schemes::secp256k1::PublicKey;

    validate_hex_len(&public_key, 33, "public key")?;
    let public_key: PublicKey = public_key.parse()?;
    let peer_id = crate::peer_id::peer_id_from_secp256k1(&public_key)?;

    Ok(SchemeResult::PeerId(PeerIdResult {
        peer_id: peer_id.to_string(),
    }))
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn execute_secp256k1_keyring_command(
    command: SchemeKeyringCommands,
    formatter: &SchemeFormatter<crate::schemes::secp256k1::Secp256k1>,
) -> Result<SchemeResult> {
    use crate::{
        Address,
        schemes::secp256k1::{PublicKey, Secp256k1, Secp256k1SignerExt},
    };

    match command {
        SchemeKeyringCommands::Generate {
            storage,
            show_secret,
        } => {
            let result = generate_key_result::<Secp256k1>(&storage, formatter, show_secret)?;
            Ok(SchemeResult::Generate(result))
        }
        SchemeKeyringCommands::Clear { storage } => {
            let storage = storage.into_storage_args();
            let result = clear_keys_command::<Secp256k1>(&storage)?;
            Ok(SchemeResult::Clear(result))
        }
        SchemeKeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context: _,
            contract,
        } => with_signer::<Secp256k1, _, _>(&storage, |signer| {
            validate_hex_len(&public_key, 33, "public key")?;
            let public_key: PublicKey = public_key.parse()?;
            let effective_prefix = prefix.or_else(|| Some(EIP191_PREFIX.to_string()));
            let message_bytes = prefixed_message(&data, &effective_prefix)?;
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());

            let signature = if let Some(contract_addr) = contract {
                let contract_bytes = hex::decode(strip_0x(&contract_addr))?;
                if contract_bytes.len() != 20 {
                    anyhow::bail!("Contract address must be 20 bytes (40 hex characters)");
                }
                let mut contract_array = [0u8; 20];
                contract_array.copy_from_slice(&contract_bytes);
                let contract_address = Address(contract_array);
                let signature = signer.sign_for_contract_with_password(
                    contract_address,
                    public_key,
                    &message_bytes,
                    password,
                )?;
                hex::encode(signature.into_pre_eip155_bytes())
            } else {
                let signature = signer.sign_with_password(public_key, &message_bytes, password)?;
                hex::encode(signature.into_pre_eip155_bytes())
            };

            Ok(SchemeResult::Sign(SignResult { signature }))
        }),
        SchemeKeyringCommands::Show {
            storage,
            key,
            show_secret,
        } => {
            let result = secp256k1_show_key(&storage, &key, show_secret, formatter)?;
            Ok(SchemeResult::List(result))
        }
        SchemeKeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => keyring_vanity::<Secp256k1KeyringOps, crate::schemes::secp256k1::Secp256k1>(
            storage,
            name,
            prefix,
            show_secret,
        ),
        SchemeKeyringCommands::Init { storage } => keyring_init::<Secp256k1KeyringOps>(storage),
        SchemeKeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => keyring_generate::<Secp256k1KeyringOps, crate::schemes::secp256k1::Secp256k1>(
            storage,
            name,
            show_secret,
        ),
        SchemeKeyringCommands::Import {
            suri,
            seed,
            private_key,
            suri_password,
            name,
            storage,
            show_secret,
        } => {
            if seed.is_some() {
                anyhow::bail!("--seed is not supported for secp256k1 import");
            }
            let name =
                name.ok_or_else(|| anyhow::anyhow!("--name is required for secp256k1 import"))?;
            if suri_password.is_some() && suri.is_none() {
                anyhow::bail!("--suri_password can only be used together with --suri");
            }
            keyring_import::<Secp256k1KeyringOps, crate::schemes::secp256k1::Secp256k1, _>(
                storage,
                name.clone(),
                show_secret,
                false,
                |keyring, key_password| {
                    if let Some(hex_key) = &private_key {
                        return Secp256k1KeyringOps::add_hex(keyring, &name, hex_key, key_password);
                    }
                    if let Some(suri) = &suri {
                        return Secp256k1KeyringOps::import_suri(
                            keyring,
                            &name,
                            suri,
                            suri_password.as_deref(),
                            key_password,
                        );
                    }
                    anyhow::bail!("either --private-key or --suri must be provided");
                },
            )
        }
        SchemeKeyringCommands::List { storage } => keyring_list::<
            Secp256k1KeyringOps,
            crate::schemes::secp256k1::Secp256k1,
        >(storage.into_storage_args()),
    }
}

#[cfg(feature = "ed25519")]
fn ed25519_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        name: crate::schemes::ed25519::Ed25519::scheme_name(),
        #[cfg(feature = "keyring")]
        handle_keyring: ed25519_handle_keyring,
        verify: ed25519_verify,
        address: ed25519_address,
        recover: None,
        #[cfg(feature = "peer-id")]
        peer_id: Some(ed25519_peer_id),
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    execute_ed25519_keyring_command(command)
}

#[cfg(feature = "ed25519")]
fn ed25519_verify(
    public_key: String,
    data: String,
    prefix: Option<String>,
    signature: String,
    _context: Option<String>,
) -> Result<SchemeResult> {
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
    Ok(substrate_result_to_scheme(res))
}

#[cfg(feature = "ed25519")]
fn ed25519_address(public_key: String, network: Option<String>) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &ed25519_descriptor(),
        SubstrateCommand::Address {
            public_key,
            network,
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(all(feature = "ed25519", feature = "peer-id"))]
fn ed25519_peer_id(public_key: String) -> Result<SchemeResult> {
    let public_key_bytes = decode_hex_array::<32>(strip_0x(&public_key), "public key")?;
    let public_key = crate::schemes::ed25519::PublicKey::from_bytes(public_key_bytes);
    let peer_id = crate::peer_id::peer_id_from_ed25519(&public_key)?;

    Ok(SchemeResult::PeerId(PeerIdResult {
        peer_id: peer_id.to_string(),
    }))
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn execute_ed25519_keyring_command(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    match command {
        SchemeKeyringCommands::Clear { storage } => {
            let storage = storage.into_storage_args();
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Clear { storage },
            )?;
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Generate {
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
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Import {
            suri,
            seed,
            private_key,
            suri_password,
            name,
            storage,
            show_secret,
        } => {
            if private_key.is_some() {
                anyhow::bail!("--private-key is not supported for ed25519");
            }
            if let Some(name) = name {
                if suri_password.is_some() && suri.is_none() {
                    anyhow::bail!("--password can only be used together with --suri");
                }
                return keyring_import::<Ed25519KeyringOps, crate::schemes::ed25519::Ed25519, _>(
                    storage,
                    name.clone(),
                    show_secret,
                    true,
                    |keyring, key_password| {
                        if let Some(seed_hex) = &seed {
                            return Ed25519KeyringOps::add_hex(
                                keyring,
                                &name,
                                seed_hex,
                                key_password,
                            );
                        }
                        if let Some(suri) = &suri {
                            return Ed25519KeyringOps::import_suri(
                                keyring,
                                &name,
                                suri,
                                suri_password.as_deref(),
                                key_password,
                            );
                        }
                        anyhow::bail!("either --seed or --suri must be provided");
                    },
                );
            }
            let res = execute_substrate_command(
                &ed25519_descriptor(),
                SubstrateCommand::Import {
                    suri,
                    seed,
                    suri_password,
                    storage,
                    show_secret,
                },
            )?;
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context: _,
            contract: _,
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
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Show {
            storage,
            key,
            show_secret,
        } => {
            let bytes = decode_hex_array::<32>(&key, "public key")?;
            let public = crate::schemes::ed25519::PublicKey::from_bytes(bytes);
            let formatter = substrate_formatter(
                crate::schemes::ed25519::Ed25519::scheme_name(),
                ed25519_key_type,
                ed25519_public_display,
            );
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());
            let result = show_key_for_public::<crate::schemes::ed25519::Ed25519>(
                &storage,
                &formatter,
                public,
                show_secret,
                password,
            )?;
            Ok(SchemeResult::List(result))
        }
        SchemeKeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => keyring_vanity::<Ed25519KeyringOps, crate::schemes::ed25519::Ed25519>(
            storage,
            name,
            prefix,
            show_secret,
        ),
        SchemeKeyringCommands::Init { storage } => keyring_init::<Ed25519KeyringOps>(storage),
        SchemeKeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => keyring_generate::<Ed25519KeyringOps, crate::schemes::ed25519::Ed25519>(
            storage,
            name,
            show_secret,
        ),
        SchemeKeyringCommands::List { storage } => keyring_list::<
            Ed25519KeyringOps,
            crate::schemes::ed25519::Ed25519,
        >(storage.into_storage_args()),
    }
}

#[cfg(feature = "sr25519")]
fn sr25519_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        name: crate::schemes::sr25519::Sr25519::scheme_name(),
        #[cfg(feature = "keyring")]
        handle_keyring: sr25519_handle_keyring,
        verify: sr25519_verify,
        address: sr25519_address,
        recover: None,
        #[cfg(feature = "peer-id")]
        peer_id: None,
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn sr25519_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    execute_sr25519_keyring_command(command)
}

#[cfg(feature = "sr25519")]
fn sr25519_verify(
    public_key: String,
    data: String,
    prefix: Option<String>,
    signature: String,
    context: Option<String>,
) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &sr25519_descriptor(),
        SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context: Some(context.unwrap_or_default()),
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(feature = "sr25519")]
fn sr25519_address(public_key: String, network: Option<String>) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &sr25519_descriptor(),
        SubstrateCommand::Address {
            public_key,
            network,
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn execute_sr25519_keyring_command(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    match command {
        SchemeKeyringCommands::Clear { storage } => {
            let storage = storage.into_storage_args();
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Clear { storage },
            )?;
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Generate {
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
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Import {
            suri,
            seed,
            private_key,
            suri_password,
            name,
            storage,
            show_secret,
        } => {
            if private_key.is_some() {
                anyhow::bail!("--private-key is not supported for sr25519");
            }
            if let Some(name) = name {
                if suri_password.is_some() && suri.is_none() {
                    anyhow::bail!("--password can only be used together with --suri");
                }
                return keyring_import::<Sr25519KeyringOps, crate::schemes::sr25519::Sr25519, _>(
                    storage,
                    name.clone(),
                    show_secret,
                    true,
                    |keyring, key_password| {
                        if let Some(seed_hex) = &seed {
                            return Sr25519KeyringOps::add_hex(
                                keyring,
                                &name,
                                seed_hex,
                                key_password,
                            );
                        }
                        if let Some(suri) = &suri {
                            return Sr25519KeyringOps::import_suri(
                                keyring,
                                &name,
                                suri,
                                suri_password.as_deref(),
                                key_password,
                            );
                        }
                        anyhow::bail!("either --seed or --suri must be provided");
                    },
                );
            }
            let res = execute_substrate_command(
                &sr25519_descriptor(),
                SubstrateCommand::Import {
                    suri,
                    seed,
                    suri_password,
                    storage,
                    show_secret,
                },
            )?;
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
            contract: _,
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
            Ok(substrate_result_to_scheme(res))
        }
        SchemeKeyringCommands::Show {
            storage,
            key,
            show_secret,
        } => {
            let bytes = decode_hex_array::<32>(&key, "public key")?;
            let public = crate::schemes::sr25519::PublicKey::from_bytes(bytes);
            let formatter = substrate_formatter(
                crate::schemes::sr25519::Sr25519::scheme_name(),
                sr25519_key_type,
                sr25519_public_display,
            );
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());
            let result = show_key_for_public::<crate::schemes::sr25519::Sr25519>(
                &storage,
                &formatter,
                public,
                show_secret,
                password,
            )?;
            Ok(SchemeResult::List(result))
        }
        SchemeKeyringCommands::Init { storage } => keyring_init::<Sr25519KeyringOps>(storage),
        SchemeKeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => keyring_vanity::<Sr25519KeyringOps, crate::schemes::sr25519::Sr25519>(
            storage,
            name,
            prefix,
            show_secret,
        ),
        SchemeKeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => keyring_generate::<Sr25519KeyringOps, crate::schemes::sr25519::Sr25519>(
            storage,
            name,
            show_secret,
        ),
        SchemeKeyringCommands::List { storage } => keyring_list::<
            Sr25519KeyringOps,
            crate::schemes::sr25519::Sr25519,
        >(storage.into_storage_args()),
    }
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
        let password = storage
            .key_password
            .as_ref()
            .map(|secret| secret.expose_secret().as_str());
        let public_key = if let Ok(public_key) = PublicKey::from_str(key) {
            public_key
        } else {
            let address = Address::from_str(key)
                .map_err(|_| anyhow::anyhow!("Invalid public key or address '{key}'"))?;
            signer
                .get_key_by_address(address)?
                .ok_or_else(|| anyhow::anyhow!("No key found for address '{key}'"))?
        };

        let info = key_info_from_public(&signer, formatter, public_key, show_secret, password)?;
        Ok(ListKeysResult { keys: vec![info] })
    })
}

#[cfg(feature = "ed25519")]
fn ed25519_descriptor() -> SubstrateDescriptor<crate::schemes::ed25519::Ed25519> {
    make_substrate_descriptor(
        substrate_formatter(
            crate::schemes::ed25519::Ed25519::scheme_name(),
            ed25519_key_type,
            ed25519_public_display,
        ),
        ed25519_key_type,
        crate::schemes::ed25519::PublicKey::from_bytes,
        crate::schemes::ed25519::Signature::from_bytes,
        |signer, public, message, _context, password| {
            Ok(signer.sign_with_password(public, message, password)?)
        },
        |public, message, signature, _context| {
            Ok(
                <crate::schemes::ed25519::Ed25519 as SignatureScheme>::verify(
                    public, message, signature,
                )?,
            )
        },
        |signature| hex::encode(signature.to_bytes()),
        |suri, password| {
            Ok(crate::schemes::ed25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        crate::address::SubstrateCryptoScheme::Ed25519,
    )
}

#[cfg(feature = "sr25519")]
fn sr25519_descriptor() -> SubstrateDescriptor<crate::schemes::sr25519::Sr25519> {
    use crate::schemes::sr25519::Sr25519SignerExt;

    make_substrate_descriptor(
        substrate_formatter(
            crate::schemes::sr25519::Sr25519::scheme_name(),
            sr25519_key_type,
            sr25519_public_display,
        ),
        sr25519_key_type,
        crate::schemes::sr25519::PublicKey::from_bytes,
        crate::schemes::sr25519::Signature::from_bytes,
        |signer, public, message, context, password| {
            let ctx = sr25519_context(&context)?;
            signer.sign_with_context_with_password(public, ctx.as_bytes(), message, password)
        },
        |public, message, signature, context| {
            let ctx = sr25519_context(&context)?;
            let signer: crate::Signer<crate::schemes::sr25519::Sr25519> = crate::Signer::memory();
            signer.verify_with_context(*public, ctx.as_bytes(), message, signature)
        },
        |signature| hex::encode(signature.to_bytes()),
        |suri, password| {
            Ok(crate::schemes::sr25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        crate::address::SubstrateCryptoScheme::Sr25519,
    )
}

#[cfg(feature = "sr25519")]
fn sr25519_context(context: &Option<String>) -> Result<&str> {
    context
        .as_deref()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| anyhow::anyhow!("sr25519 requires a non-empty signing context (--context)"))
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
struct Secp256k1KeyringOps;

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl KeyringOps for Secp256k1KeyringOps {
    type Keyring = crate::schemes::secp256k1::keyring::Keyring;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_SECP
    }

    fn load(path: PathBuf) -> Result<Self::Keyring> {
        Self::Keyring::load(path)
    }

    fn memory() -> Self::Keyring {
        Self::Keyring::memory()
    }

    fn create(
        keyring: &mut Self::Keyring,
        name: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) = keyring.create(name, password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(private_key.to_string()),
        })
    }

    fn add_hex(
        keyring: &mut Self::Keyring,
        name: &str,
        hex: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let keystore = keyring.add_hex(name, hex, password)?;
        let secret = keystore.private_key_with_password(password)?.to_string();
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(secret),
        })
    }

    fn import_suri(
        keyring: &mut Self::Keyring,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) =
            keyring.import_suri(name, suri, suri_password, encryption_password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(private_key.to_string()),
        })
    }

    fn list(keyring: &Self::Keyring) -> Result<Vec<KeyringEntry>> {
        Ok(keyring
            .list()
            .iter()
            .map(|ks| KeyringEntry {
                public_key: ks.public_key.clone(),
                address: ks.address.clone(),
                key_type: ks.meta.key_type.clone(),
                name: ks.name.clone(),
                secret: None,
            })
            .collect())
    }

    fn vanity(
        keyring: &mut Self::Keyring,
        name: &str,
        prefix: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let normalized = prefix.trim();
        let normalized = normalized
            .strip_prefix("0x")
            .unwrap_or(normalized)
            .to_ascii_lowercase();
        if !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("Prefix must be hexadecimal");
        }

        let target = normalized.clone();
        let mut attempts = 0usize;
        let private_key = loop {
            if attempts >= MAX_VANITY_ATTEMPTS {
                anyhow::bail!("vanity search exceeded maximum attempts ({MAX_VANITY_ATTEMPTS})");
            }
            let candidate = crate::schemes::secp256k1::PrivateKey::random();
            let address_hex = candidate.public_key().to_address().to_hex();
            if target.is_empty() || address_hex.starts_with(&target) {
                break candidate;
            }
            attempts += 1;
        };
        let keystore = keyring.add(name, private_key.clone(), password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(private_key.to_string()),
        })
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
struct Ed25519KeyringOps;

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl KeyringOps for Ed25519KeyringOps {
    type Keyring = crate::schemes::ed25519::keyring::Keyring;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_ED
    }

    fn load(path: PathBuf) -> Result<Self::Keyring> {
        Self::Keyring::load(path)
    }

    fn memory() -> Self::Keyring {
        Self::Keyring::memory()
    }

    fn create(
        keyring: &mut Self::Keyring,
        name: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) = keyring.create(name, password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }

    fn add_hex(
        keyring: &mut Self::Keyring,
        name: &str,
        hex: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let keystore = keyring.add_hex(name, hex, password)?;
        let secret = keystore.private_key_with_password(password)?.to_bytes();
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(hex::encode(secret)),
        })
    }

    fn import_suri(
        keyring: &mut Self::Keyring,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) =
            keyring.import_suri(name, suri, suri_password, encryption_password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }

    fn list(keyring: &Self::Keyring) -> Result<Vec<KeyringEntry>> {
        Ok(keyring
            .list()
            .iter()
            .map(|ks| KeyringEntry {
                public_key: ks.public_key.clone(),
                address: ks.address.clone(),
                key_type: ks.meta.key_type.clone(),
                name: ks.name.clone(),
                secret: None,
            })
            .collect())
    }

    fn vanity(
        keyring: &mut Self::Keyring,
        name: &str,
        prefix: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let mut attempts = 0usize;
        let private_key = loop {
            if attempts >= MAX_VANITY_ATTEMPTS {
                anyhow::bail!("vanity search exceeded maximum attempts ({MAX_VANITY_ATTEMPTS})");
            }
            let candidate = crate::schemes::ed25519::PrivateKey::random();
            let address = candidate.public_key().to_address()?;
            if prefix.is_empty() || address.as_ss58().starts_with(prefix) {
                break candidate;
            }
            attempts += 1;
        };
        let keystore = keyring.add(name, private_key.clone(), password)?;
        Ok(KeyringEntry {
            public_key: keystore.public_key.clone(),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
struct Sr25519KeyringOps;

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl KeyringOps for Sr25519KeyringOps {
    type Keyring = crate::schemes::sr25519::keyring::Keyring;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_SR
    }

    fn load(path: PathBuf) -> Result<Self::Keyring> {
        Self::Keyring::load(path)
    }

    fn memory() -> Self::Keyring {
        Self::Keyring::memory()
    }

    fn create(
        keyring: &mut Self::Keyring,
        name: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) = keyring.create(name, password.map(|p| p.as_bytes()))?;
        Ok(KeyringEntry {
            public_key: hex::encode(private_key.public_key().to_bytes()),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.meta.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }

    fn add_hex(
        keyring: &mut Self::Keyring,
        name: &str,
        hex: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let bytes = hex::decode(strip_0x(hex))?;
        if bytes.len() != 32 {
            anyhow::bail!("Seed must be 32 bytes (64 hex characters)");
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let private_key = crate::schemes::sr25519::PrivateKey::from_seed(seed)?;
        let keystore = keyring.add(name, private_key.clone(), password.map(|p| p.as_bytes()))?;
        Ok(KeyringEntry {
            public_key: hex::encode(private_key.public_key().to_bytes()),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.meta.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }

    fn import_suri(
        keyring: &mut Self::Keyring,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let private_key = crate::schemes::sr25519::PrivateKey::from_suri(suri, suri_password)?;
        let keystore = keyring.add(
            name,
            private_key.clone(),
            encryption_password.map(|p| p.as_bytes()),
        )?;
        Ok(KeyringEntry {
            public_key: hex::encode(private_key.public_key().to_bytes()),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.meta.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }

    fn list(keyring: &Self::Keyring) -> Result<Vec<KeyringEntry>> {
        Ok(keyring
            .list()
            .iter()
            .map(|ks| KeyringEntry {
                public_key: ks
                    .public_key()
                    .map(hex::encode)
                    .unwrap_or_else(|_| "<unknown>".to_string()),
                address: ks.address.clone(),
                key_type: ks.meta.key_type.clone(),
                name: ks.meta.name.clone(),
                secret: None,
            })
            .collect())
    }

    fn vanity(
        keyring: &mut Self::Keyring,
        name: &str,
        prefix: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let mut attempts = 0usize;
        let private_key = loop {
            if attempts >= MAX_VANITY_ATTEMPTS {
                anyhow::bail!("vanity search exceeded maximum attempts ({MAX_VANITY_ATTEMPTS})");
            }
            let candidate = crate::schemes::sr25519::PrivateKey::random();
            let address = candidate.public_key().to_address()?;
            if prefix.is_empty() || address.as_ss58().starts_with(prefix) {
                break candidate;
            }
            attempts += 1;
        };
        let keystore = keyring.add(name, private_key.clone(), password.map(|p| p.as_bytes()))?;
        Ok(KeyringEntry {
            public_key: hex::encode(private_key.public_key().to_bytes()),
            address: keystore.address.clone(),
            key_type: keystore.meta.key_type.clone(),
            name: keystore.meta.name.clone(),
            secret: Some(hex::encode(private_key.to_bytes())),
        })
    }
}
