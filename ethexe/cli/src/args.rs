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

use crate::{
    config,
    params::{NetworkParams, PrometheusParams, RpcParams},
};
use anyhow::{anyhow, bail};
use clap::{Parser, Subcommand};
use ethexe_ethereum::Ethereum;
use ethexe_signer::Address;
use gprimitives::CodeId;
use serde::Deserialize;
use std::{fs, num::NonZero, path::PathBuf};

#[derive(Clone, Debug, Parser, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Name of node for telemetry
    #[arg(long, default_value = "test")]
    pub node_name: String,

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
    #[arg(long = "ethereum-router-address", alias = "router")]
    pub ethereum_router_address: Option<String>,

    /// Path to ChainSpec toml.
    #[arg(long, short = 'c')]
    pub chain_spec: Option<String>,

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

    /// Max depth to discover last commitment.
    #[arg(long = "max-depth")]
    pub max_commitment_depth: Option<u32>,

    /// Block time in seconds (approximate).
    /// Ethexe uses it to estimate inner timeouts.
    #[arg(long, default_value = "12")]
    pub block_time: u64,

    /// Amount of physical threads tokio runtime will use for program processing.
    ///
    /// The default value is the number of cores available to the system.
    #[arg(long = "worker-threads")]
    pub worker_threads_override: Option<NonZero<u8>>,

    /// Amount of virtual threads (workers) for programs processing.
    #[arg(long, default_value = "16")]
    pub virtual_threads: NonZero<u8>,

    /// Run a temporary node.
    ///
    /// A temporary directory will be created to store the configuration and will be deleted
    /// at the end of the process.
    ///
    /// Note: the directory is random per process execution. This directory is used as base path
    /// which includes: database, node key and keystore.
    #[arg(long, conflicts_with = "base_path")]
    #[serde(default)]
    pub tmp: bool,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub network_params: Option<NetworkParams>,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub prometheus_params: Option<PrometheusParams>,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub rpc_params: Option<RpcParams>,

    #[command(subcommand)]
    pub extra_command: Option<ExtraCommands>,
}

// CLI args when `.ethexe.toml` is used
#[derive(Clone, Debug, Parser, Deserialize)]
#[command(version, about, long_about = None)]
pub struct ArgsOnConfig {
    #[command(subcommand)]
    pub extra_command: Option<ExtraCommands>,
}

#[derive(Clone, Debug, Subcommand, Deserialize)]
pub enum ExtraCommands {
    GenerateKey {
        /// Print only secp256k1 public key
        #[arg(long, conflicts_with = "ethereum")]
        secp256k1: bool,
        /// Print only Ethereum address
        #[arg(long, conflicts_with = "secp256k1")]
        ethereum: bool,
    },
    ListKeys,
    ClearKeys,
    InsertKey(InsertKeyArgs),
    Sign(SigningArgs),
    UploadCode(UploadCodeArgs),
    CreateProgram(CreateProgramArgs),
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
pub struct CreateProgramArgs {
    code_id: String,
}

// TODO (breathx): support message sending here.

impl ExtraCommands {
    pub async fn run(&self, config: &config::Config) -> anyhow::Result<()> {
        let signer = ethexe_signer::Signer::new(config.key_path.clone())?;

        // TODO: for better UI, we must split commands processing for ones that require ethereum and ones that don't
        let maybe_sender_address = config
            .sender_address
            .as_ref()
            .and_then(|addr| addr.parse::<Address>().ok());
        let maybe_ethereum = if let Some(sender_address) = maybe_sender_address {
            Ethereum::new(
                &config.ethereum_rpc,
                config.ethereum_router_address,
                signer.clone(),
                sender_address,
            )
            .await
            .ok()
        } else {
            None
        };

        match self {
            ExtraCommands::GenerateKey {
                secp256k1,
                ethereum,
            } => {
                let new_pub = signer.generate_key()?;

                if *secp256k1 {
                    println!("{new_pub}");
                } else if *ethereum {
                    println!("{}", new_pub.to_address())
                } else {
                    println!("New public key stored: {}", new_pub);
                    println!("Ethereum address: {}", new_pub.to_address());
                }
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
                    bail!("No keys found, please generate a key first");
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

                let Some((sender_address, ethexe_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    bail!("please provide signer address");
                };

                println!(
                    "Uploading {} to Ethereum from {sender_address}...",
                    path.display(),
                );

                let router = ethexe_ethereum.router();

                let code = fs::read(path)?;
                let (tx, code_id) = router.request_code_validation_with_sidecar(&code).await?;

                println!("Completed in transaction {tx:?}");
                println!("Waiting for approval of code id {code_id}...");

                let valid = router.wait_code_validation(code_id).await?;

                if valid {
                    println!("Now you can create program from code id {code_id}!");
                } else {
                    bail!("Given code is invalid and failed validation");
                }
            }

            ExtraCommands::CreateProgram(ref create_program_args) => {
                let code_id: CodeId = create_program_args
                    .code_id
                    .parse()
                    .map_err(|err| anyhow!("failed to parse code id: {err}"))?;

                let salt = rand::random();

                let Some((sender_address, ethexe_ethereum)) =
                    maybe_sender_address.zip(maybe_ethereum)
                else {
                    bail!("please provide signer address");
                };

                println!("Creating program on Ethereum from code id {code_id} and address {sender_address}...",);

                let router = ethexe_ethereum.router();

                let (tx, actor_id) = router.create_program(code_id, salt).await?;

                println!("Completed in transaction {tx:?}");
                println!(
                    "Program address on Ethereum {:?}",
                    actor_id.to_address_lossy()
                );

                // TODO: handle events from commitTransitions
            }
        }

        Ok(())
    }
}
