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

use super::utils;
use crate::params::Params;
use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::{Parser, Subcommand};
use ethexe_ethereum::Ethereum;
use ethexe_signer::{Address, Signer};
use gprimitives::H256;
use std::{fs, path::PathBuf};

#[derive(Debug, Parser)]
pub struct TxCommand {
    #[arg(long)]
    pub key_store: Option<PathBuf>,

    #[arg(long, alias = "eth-rpc")]
    pub ethereum_rpc: Option<String>,

    #[arg(long, alias = "eth-router")]
    pub ethereum_router: Option<String>,

    #[arg(long)]
    pub sender: Option<String>,

    #[command(subcommand)]
    pub command: TxSubcommand,
}

impl TxCommand {
    pub fn with_params(mut self, params: Params) -> Self {
        self.key_store = self
            .key_store
            .take()
            .or_else(|| Some(params.node.unwrap_or_default().keys_dir()));

        self.ethereum_rpc = self.ethereum_rpc.take().or_else(|| {
            params
                .ethereum
                .as_ref()
                .and_then(|p| p.ethereum_rpc.clone())
        });

        self.ethereum_router = self.ethereum_router.take().or_else(|| {
            params
                .ethereum
                .as_ref()
                .and_then(|p| p.ethereum_router.clone())
        });

        self
    }

    pub async fn exec(self) -> Result<()> {
        let key_store = self.key_store.expect("must never be empty after merging");

        let signer = Signer::new(key_store).with_context(|| "failed to create signer")?;

        let rpc = self
            .ethereum_rpc
            .ok_or_else(|| anyhow!("missing `ethereum-rpc`"))?;

        let router_addr = self
            .ethereum_router
            .ok_or_else(|| anyhow!("missing `ethereum-router`"))?
            .parse()
            .with_context(|| "invalid `ethereum-router`")?;

        let sender = self
            .sender
            .ok_or_else(|| anyhow!("missing `sender`"))?
            .parse()
            .with_context(|| "invalid `sender`")?;

        let ethereum = Ethereum::new(&rpc, router_addr, signer, sender)
            .await
            .with_context(|| "failed to create Ethereum client")?;

        let router = ethereum.router();
        let router_query = router.query();

        match self.command {
            TxSubcommand::Create { code_id, salt } => {
                let code_id = code_id
                    .parse()
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| "invalid `code-id`")?;

                let salt = salt
                    .map(|s| s.parse())
                    .transpose()
                    .with_context(|| "invalid `salt`")?
                    .unwrap_or_else(H256::random);

                println!("Creating program on Ethereum from code id {code_id}");

                let (tx, actor_id) = router
                    .create_program(code_id, salt)
                    .await
                    .with_context(|| "failed to create program")?;

                println!("Completed in transaction {tx:?}");
                println!(
                    "Program address on Ethereum {:?}",
                    actor_id.to_address_lossy()
                );
            }
            // TODO (breathx): impl batching.
            TxSubcommand::Message {
                mirror,
                payload,
                value,
                approve,
                watch,
            } => {
                let mirror_addr: Address = mirror.parse().with_context(|| "invalid `mirror`")?;

                let payload =
                    utils::hex_str_to_vec(payload).with_context(|| "invalid `payload`")?;

                let maybe_code_id = router_query
                    .program_code_id(mirror_addr.into())
                    .await
                    .with_context(|| "failed to check if mirror in known by router")?;

                ensure!(
                    maybe_code_id.is_some(),
                    "Given mirror address is not recognized by router"
                );

                if value != 0 && approve {
                    // TODO (breathx): add separator for tokens; maybe impl gprimitive for it.
                    println!("Approving {value} value of WVara on Ethereum for {mirror_addr}");

                    let tx = router
                        .wvara()
                        .approve(mirror_addr.0.into(), value)
                        .await
                        .with_context(|| "failed to approve wvara")?;

                    println!("Completed in transaction {tx:?}");
                }

                println!("Sending message on Ethereum to {mirror_addr}");

                let mirror = ethereum.mirror(mirror_addr);

                let (tx, message_id) = mirror
                    .send_message(payload, value)
                    .await
                    .with_context(|| "failed to send message to mirror")?;

                println!("Completed in transaction {tx:?}");
                println!("Message with id {message_id} successfully sent");

                if watch {
                    unimplemented!("Watching reply is not yet implemented");
                }
            }
            TxSubcommand::Upload { path_to_wasm } => {
                let code =
                    fs::read(&path_to_wasm).with_context(|| "failed to read wasm from file")?;

                println!("Uploading {} to Ethereum", path_to_wasm.display(),);

                let (tx, code_id) = router
                    .request_code_validation_with_sidecar(&code)
                    .await
                    .with_context(|| "failed to request code validation")?;

                println!("Completed in transaction {tx:?}");
                println!("Waiting for approval of code id {code_id}...");

                let valid = router
                    .wait_code_validation(code_id)
                    .await
                    .with_context(|| "failed to wait for code validation")?;

                if valid {
                    println!("Now you can create program from code id {code_id}!");
                } else {
                    bail!("Given code is invalid and failed validation");
                }
            }
        }

        Ok(())
    }
}

// TODO (breathx): impl reply, value claim and exec balance top up with watch.
#[derive(Debug, Subcommand)]
pub enum TxSubcommand {
    Create {
        #[arg(short, long, alias = "code")]
        code_id: String,
        #[arg(short, long)]
        salt: Option<String>,
    },
    Message {
        #[arg(short, long)]
        mirror: String,
        #[arg(short, long)]
        payload: String,
        #[arg(short, long, default_value = "0")]
        value: u128,
        #[arg(short, long, default_value = "false")]
        approve: bool,
        #[arg(short, long, default_value = "false")]
        watch: bool,
    },
    Upload {
        #[arg()]
        path_to_wasm: PathBuf,
    },
}
