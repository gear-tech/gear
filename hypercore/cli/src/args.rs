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

use clap::{Parser, Subcommand};
use hypercore_ethereum::{Router, Sender};
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
        default_value = "0x9F1291e0DE8F29CC7bF16f7a8cb39e7Aebf33B9b"
    )]
    pub ethereum_router_address: String,

    /// Address of Ethereum Program contract
    #[arg(
        long = "ethereum-program-address",
        alias = "program",
        default_value = "0x23a4FC5f430a7c3736193B852Ad5191c7EC01037"
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
    Sign(SigningArgs),
    UploadCode(UploadCodeArgs),
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct SigningArgs {
    message: String,
}

#[derive(Clone, Debug, Deserialize, Parser)]
pub struct UploadCodeArgs {
    path: PathBuf,
    sender: String,
}

impl ExtraCommands {
    pub async fn run(&self, config: &config::Config) -> anyhow::Result<()> {
        let signer = hypercore_signer::Signer::new(config.key_path.clone())?;

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
                    println!("No keys found, please generate a key first");
                    return Ok(());
                }

                println!("Signing with all ({}) keys:", key_list.len());

                for key in &key_list {
                    println!("Ethereum Address: {}, public: {}", key.to_address(), key);
                    println!("Signature: {}", &signer.sign(*key, message.as_bytes())?);
                    println!("--------------------------------------------");
                }
            }

            ExtraCommands::UploadCode(ref upload_code_args) => {
                let path = &upload_code_args.path;
                let sender_address = Address::from_hex(
                    if let Some(sender) = upload_code_args.sender.strip_prefix("0x") {
                        sender
                    } else {
                        &upload_code_args.sender
                    },
                )?;

                let Some(key) = signer.get_key_by_addr(sender_address)? else {
                    println!("No key found for 0x{}", sender_address.to_hex());
                    return Ok(());
                };

                println!(
                    "Uploading {} to Ethereum from 0x{}...",
                    path.display(),
                    sender_address.to_hex(),
                );

                let router = Router::new(
                    &config.ethereum_router_address,
                    &config.ethereum_rpc,
                    Sender::new(signer.clone(), key),
                )
                .await?;

                let code = fs::read(path)?;
                let tx = router.upload_code_with_sidecar(&code).await?;

                println!("Completed in transaction {tx:?}");
            }
        }

        Ok(())
    }
}
