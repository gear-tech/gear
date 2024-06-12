// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! CLI arguments in one place.

use anyhow::Result;
use clap::{Parser, Subcommand};
use gprimitives::ActorId;
use hypercore_ethereum::HypercoreEthereum;
use hypercore_signer::Address;
use serde::Deserialize;
use std::{fs, path::PathBuf};

use crate::params::NetworkParams;

use crate::config;

#[derive(Clone, Debug, Parser, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// URL of Ethereum RPC endpoint
    #[arg(
        long = "ethereum-rpc",
        alias = "rpc",
        default_value = "http://localhost:8545"
    )]
    pub ethereum_rpc: String,

    /// URL of Ethereum Beacon Chain RPC endpoint
    #[arg(
        long = "ethereum-beacon-rpc",
        alias = "beacon-rpc",
        default_value = "http://localhost:5052"
    )]
    pub ethereum_beacon_rpc: String,

    /// Address of Ethereum Router contract
    #[arg(
        long = "ethereum-router-address",
        alias = "router",
        default_value = "0xA2B95aC9aA1E821126Af6FBd65E93a23b2f85C2e"
    )]
    pub ethereum_router_address: String,

    /// Address of Ethereum Program contract
    #[arg(
        long = "ethereum-program-address",
        alias = "program",
        default_value = "0xDB4fE5d350a1E84be106Bc8b8f01AB9037fba2A0"
    )]
    pub ethereum_program_address: String,

    /// Base path where application settings are stored
    #[arg(long, short = 'd', value_name = "PATH")]
    pub base_path: Option<PathBuf>,

    /// Sequencer key, if intended to run node in sequencer mode.
    #[arg(long = "sequencer-key")]
    pub sequencer_key: Option<String>,

    /// Validator (processor) key, if intended to run node in validator mode.
    #[arg(long = "validator-key")]
    pub validator_key: Option<String>,

    /// Sender address, if intended to send Ethereum transaction.
    #[arg(long = "validator-address")]
    pub sender_address: Option<String>,

    /// Run a temporary node.
    ///
    /// A temporary directory will be created to store the configuration and will be deleted
    /// at the end of the process.
    ///
    /// Note: the directory is random per process execution. This directory is used as base path
    /// which includes: database, node key and keystore.
    #[arg(long, conflicts_with = "base_path")]
    pub tmp: bool,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub network_params: NetworkParams,

    #[command(subcommand)]
    pub extra_command: Option<ExtraCommands>,
}

#[derive(Clone, Debug, Subcommand, Deserialize)]
pub enum ExtraCommands {
    GenerateKey,
    ListKeys,
    ClearKeys,
    InsertKey(InsertKeyArgs),
    Sign(SigningArgs),
    UploadCode(UploadCodeArgs),
    SetProgram,
    AddValidators(AddValidatorsArgs),
    RemoveValidators(RemoveValidatorsArgs),
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct SigningArgs {
    message: String,
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct InsertKeyArgs {
    key_uri: String,
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct UploadCodeArgs {
    path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct AddValidatorsArgs {
    validators: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct RemoveValidatorsArgs {
    validators: Vec<String>,
}

impl ExtraCommands {
    pub async fn run(&self, config: &config::Config) -> anyhow::Result<()> {
        let signer = hypercore_signer::Signer::new(config.key_path.clone())?;

        let maybe_sender_address = config
            .sender_address
            .as_ref()
            .and_then(|addr| addr.parse::<Address>().ok());
        let maybe_ethereum = if let Some(sender_address) = maybe_sender_address {
            Some(
                HypercoreEthereum::new(
                    &config.ethereum_rpc,
                    config.ethereum_router_address.parse()?,
                    signer.clone(),
                    sender_address,
                )
                .await?,
            )
        } else {
            None
        };

        match self {
            ExtraCommands::GenerateKey => {
                let new_pub = signer.generate_key()?;

                println!("New public key stored: {}", new_pub);
                println!("Ethereum address: {}", new_pub.to_address());
            }

            ExtraCommands::ClearKeys => {
                println!("Total {} keys will be cleared: ", signer.list_keys()?.len());
                signer.clear_keys()?;
                println!("Total {} keys left: ", signer.list_keys()?.len());
            }

            ExtraCommands::ListKeys => {
                let key_list = signer.list_keys()?;

                for key in &key_list {
                    println!("Ethereum Address: {}, public: {}", key.to_address(), key);
                }

                println!("Total {}", key_list.len())
            }

            ExtraCommands::Sign(ref signing_args) => {
                let message = &signing_args.message;

                let key_list = signer.list_keys()?;

                if key_list.is_empty() {
                    anyhow::bail!("No keys found, please generate a key first");
                }

                println!("Signing with all ({}) keys:", key_list.len());

                for key in &key_list {
                    println!("Ethereum Address: {}, public: {}", key.to_address(), key);
                    println!("Signature: {}", &signer.sign(*key, message.as_bytes())?);
                    println!("--------------------------------------------");
                }
            }

            ExtraCommands::InsertKey(ref insert_key_args) => {
                let private_hex = insert_key_args.key_uri.parse()?;
                let pub_key = signer.add_key(private_hex)?;

                println!("Key inserted: {}", pub_key);
                println!("Ethereum address: {}", pub_key.to_address());
            }

            ExtraCommands::UploadCode(ref upload_code_args) => {
                let path = &upload_code_args.path;

                let Some((sender_address, hypercore_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    anyhow::bail!("please provide signer address");
                };

                println!(
                    "Uploading {} to Ethereum from {sender_address}...",
                    path.display(),
                );

                let code = fs::read(path)?;
                let tx = hypercore_ethereum
                    .router()
                    .upload_code_with_sidecar(&code)
                    .await?;

                println!("Completed in transaction {tx:?}");
            }

            ExtraCommands::SetProgram => {
                let program_impl: Address = config.ethereum_program_address.parse()?;

                let Some((sender_address, hypercore_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    anyhow::bail!("please provide signer address");
                };

                println!("Setting program {program_impl} for Router from {sender_address}...");

                let tx = hypercore_ethereum
                    .router()
                    .set_program({
                        let mut actor_id = [0; 32];
                        actor_id[12..].copy_from_slice(&program_impl.0);
                        ActorId::new(actor_id)
                    })
                    .await?;
                println!("Completed in transaction {tx:?}");
            }

            ExtraCommands::AddValidators(ref add_validators_args) => {
                let validator_addresses = add_validators_args
                    .validators
                    .iter()
                    .map(|validator| validator.parse::<Address>())
                    .collect::<Result<Vec<_>>>()?;

                let validators = validator_addresses
                    .into_iter()
                    .map(|validator| {
                        let mut actor_id = [0; 32];
                        actor_id[12..].copy_from_slice(&validator.0);
                        ActorId::new(actor_id)
                    })
                    .collect();

                let Some((sender_address, hypercore_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    anyhow::bail!("please provide signer address");
                };

                println!("Adding validators for Router from {sender_address}...");

                let tx = hypercore_ethereum
                    .router()
                    .add_validators(validators)
                    .await?;
                println!("Completed in transaction {tx:?}");
            }

            ExtraCommands::RemoveValidators(ref remove_validators_args) => {
                let validator_addresses = remove_validators_args
                    .validators
                    .iter()
                    .map(|validator| validator.parse::<Address>())
                    .collect::<Result<Vec<_>>>()?;

                let validators = validator_addresses
                    .into_iter()
                    .map(|validator| {
                        let mut actor_id = [0; 32];
                        actor_id[12..].copy_from_slice(&validator.0);
                        ActorId::new(actor_id)
                    })
                    .collect();

                let Some((sender_address, hypercore_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    anyhow::bail!("please provide signer address");
                };

                println!("Removing validators for Router from {sender_address}...");

                let tx = hypercore_ethereum
                    .router()
                    .remove_validators(validators)
                    .await?;
                println!("Completed in transaction {tx:?}");
            }
        }

        Ok(())
    }
}
