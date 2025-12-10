// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::params::Params;
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Parser, Subcommand};
use ethexe_common::Address;
use ethexe_ethereum::Ethereum;
use ethexe_signer::Signer;
use gprimitives::{CodeId, H256};
use sp_core::Bytes;
use std::{fs, path::PathBuf};

/// Submit a transaction.
#[derive(Debug, Parser)]
pub struct TxCommand {
    /// Primary key store to use (use to override generation from base path).
    #[arg(long)]
    pub key_store: Option<PathBuf>,

    /// Ethereum RPC endpoint to use.
    #[arg(long, alias = "eth-rpc")]
    pub ethereum_rpc: Option<String>,

    /// Ethereum router address to use.
    #[arg(long, alias = "eth-router")]
    pub ethereum_router: Option<Address>,

    /// Sender address or public key to use. Must have a corresponding private key in the key store.
    #[arg(long)]
    pub sender: Option<Address>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: TxSubcommand,
}

impl TxCommand {
    /// Merge the command with the provided params.
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

        self.ethereum_router = self
            .ethereum_router
            .take()
            .or_else(|| params.ethereum.as_ref().and_then(|p| p.ethereum_router));

        self
    }

    /// Execute the command.
    pub fn exec(self) -> Result<()> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(self.exec_inner())
    }

    async fn exec_inner(self) -> Result<()> {
        let key_store = self.key_store.expect("must never be empty after merging");

        let signer = Signer::fs(key_store);

        let rpc = self
            .ethereum_rpc
            .ok_or_else(|| anyhow!("missing `ethereum-rpc`"))?;

        let router_addr = self
            .ethereum_router
            .ok_or_else(|| anyhow!("missing `ethereum-router`"))?;

        let sender = self.sender.ok_or_else(|| anyhow!("missing `sender`"))?;

        let ethereum = Ethereum::new(&rpc, router_addr.into(), signer, sender)
            .await
            .with_context(|| "failed to create Ethereum client")?;

        let router = ethereum.router();
        let router_query = router.query();

        match self.command {
            TxSubcommand::Upload {
                path_to_wasm,
                watch,
            } => {
                let code =
                    fs::read(&path_to_wasm).with_context(|| "failed to read wasm from file")?;

                println!("Uploading {} to Ethereum", path_to_wasm.display());

                let pending_builder = router
                    .request_code_validation_with_sidecar(&code)
                    .await
                    .with_context(|| "failed to create code validation request")?;

                let (tx, code_id) = pending_builder
                    .send()
                    .await
                    .with_context(|| "failed to request code validation")?;

                println!("Completed in transaction {tx:?}");

                if watch {
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
            TxSubcommand::Create {
                code_id,
                salt,
                initializer,
            } => {
                let salt = salt.unwrap_or_else(H256::random);
                let override_initializer = initializer.map(Into::into);

                println!("Creating program on Ethereum from code id {code_id} and salt {salt:?}");

                let (tx, actor_id) = router
                    .create_program(code_id, salt, override_initializer)
                    .await
                    .with_context(|| "failed to create program")?;

                println!("Completed in transaction {tx:?}");
                println!(
                    "Program address on Ethereum {:?}",
                    actor_id.to_address_lossy()
                );
            }
            TxSubcommand::CreateWithAbi {
                code_id,
                salt,
                initializer,
                abi_interface,
            } => {
                let salt = salt.unwrap_or_else(H256::random);
                let override_initializer = initializer.map(Into::into);

                println!("Creating program on Ethereum from code id {code_id} and salt {salt:?}");

                let (tx, actor_id) = router
                    .create_program_with_abi_interface(
                        code_id,
                        salt,
                        override_initializer,
                        abi_interface.into(),
                    )
                    .await
                    .with_context(|| "failed to create program")?;

                println!("Completed in transaction {tx:?}");
                println!(
                    "Program address on Ethereum {:?}",
                    actor_id.to_address_lossy()
                );
            }
            TxSubcommand::Query { mirror } => {
                let maybe_code_id = router_query
                    .program_code_id(mirror.into())
                    .await
                    .with_context(|| "failed to check if mirror in known by router")?;

                ensure!(
                    maybe_code_id.is_some(),
                    "Given mirror address is not recognized by router"
                );

                println!("Querying state of mirror on Ethereum at {mirror}");

                let mirror = ethereum.mirror(mirror);
                let mirror_query = mirror.query();

                let router = mirror_query.router().await?;
                let state_hash = mirror_query.state_hash().await?;
                let nonce = mirror_query.nonce().await?;
                let exited = mirror_query.exited().await?;
                let inheritor = mirror_query.inheritor().await?;
                let initializer = mirror_query.initializer().await?;
                let balance = mirror.get_balance().await?;

                println!("Mirror state:");
                println!("  Router:          {router:?}");
                println!("  State hash:      {state_hash:?}");
                println!("  Nonce:           {nonce}");
                println!("  Exited:          {exited}");
                println!("  Inheritor:       {inheritor}",);
                println!("  Initializer:     {initializer}",);
                println!("  ETH Balance:     {balance} wei");
                // TODO: format balance as wei and ETH
            }
            TxSubcommand::OwnedBalanceTopUp { mirror, value } => {
                let maybe_code_id = router_query
                    .program_code_id(mirror.into())
                    .await
                    .with_context(|| "failed to check if mirror in known by router")?;

                ensure!(
                    maybe_code_id.is_some(),
                    "Given mirror address is not recognized by router"
                );

                // TODO: format balance as wei and ETH
                println!(
                    "Topping up owned balance of mirror on Ethereum at {mirror} by {value} wei"
                );

                let mirror = ethereum.mirror(mirror);

                let tx = mirror
                    .owned_balance_top_up(value)
                    .await
                    .with_context(|| "failed to top up owned balance of mirror")?;

                println!("Completed in transaction {tx:?}");
                println!("Owned balance of mirror successfully topped up");
            }
            TxSubcommand::ExecutableBalanceTopUp {
                mirror,
                value,
                approve,
            } => {
                let maybe_code_id = router_query
                    .program_code_id(mirror.into())
                    .await
                    .with_context(|| "failed to check if mirror in known by router")?;

                ensure!(
                    maybe_code_id.is_some(),
                    "Given mirror address is not recognized by router"
                );

                // TODO: format balance as value and WVARA
                println!(
                    "Topping up executable balance of mirror on Ethereum at {mirror} by {value} WVARA"
                );

                let mirror = ethereum.mirror(mirror);

                if value != 0 && approve {
                    ethereum
                        .router()
                        .wvara()
                        .approve(mirror.address().into(), value)
                        .await?;
                }

                let tx = mirror
                    .executable_balance_top_up(value)
                    .await
                    .with_context(|| "failed to top up executable balance of mirror")?;

                println!("Completed in transaction {tx:?}");
                println!("Executable balance of mirror successfully topped up");
            }
            TxSubcommand::SendMessage {
                mirror,
                payload,
                value,
                call_reply,
                watch,
            } => {
                let maybe_code_id = router_query
                    .program_code_id(mirror.into())
                    .await
                    .with_context(|| "failed to check if mirror in known by router")?;

                ensure!(
                    maybe_code_id.is_some(),
                    "Given mirror address is not recognized by router"
                );

                println!("Sending message on Ethereum to {mirror}");

                let mirror = ethereum.mirror(mirror);

                let (tx, message_id) = mirror
                    .send_message(payload.0, value, call_reply)
                    .await
                    .with_context(|| "failed to send message to mirror")?;

                println!("Completed in transaction {tx:?}");
                println!("Message with id {message_id} successfully sent");

                if watch {
                    unimplemented!("Watching reply is not yet implemented");
                }
            }
        }

        Ok(())
    }
}

// TODO (breathx): impl reply, value claim and exec balance top up with watch.
// TODO (breathx) submit offchain txs
/// Available transaction to submit.
#[derive(Debug, Subcommand)]
pub enum TxSubcommand {
    /// Upload Wasm code to Ethereum: request its validation for further program creation.
    Upload {
        /// Path to the Wasm file.
        #[arg()]
        path_to_wasm: PathBuf,
        /// Flag to watch for code validation result. If false, command will do not wait for validation.
        #[arg(short, long, default_value = "false")]
        watch: bool,
    },
    /// Create new mirror program on Ethereum.
    Create {
        /// Wasm code id to use.
        #[arg()]
        code_id: CodeId,
        /// Salt to use for program id generation. If not provided, random is used.
        #[arg(short, long)]
        salt: Option<H256>,
        /// Override initializer address. If not provided, sender is used.
        #[arg(short, long)]
        initializer: Option<Address>,
    },
    /// Create new mirror program on Ethereum with ABI interface.
    CreateWithAbi {
        /// Wasm code id to use.
        #[arg()]
        code_id: CodeId,
        /// Salt to use for program id generation. If not provided, random is used.
        #[arg(short, long)]
        salt: Option<H256>,
        /// Override initializer address. If not provided, sender is used.
        #[arg(short, long)]
        initializer: Option<Address>,
        /// ABI interface address. Mirror contract will be stub for all methods so that it will be possible
        /// to interact with the Sails contract via etherscan.
        #[arg()]
        abi_interface: Address,
    },
    /// Query mirror state on Ethereum.
    Query {
        /// Mirror address.
        #[arg()]
        mirror: Address,
    },
    /// Top up owned balance of mirror on Ethereum.
    OwnedBalanceTopUp {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// ETH value to top up.
        #[arg()]
        value: u128,
    },
    /// Top up executable balance of mirror on Ethereum.
    ExecutableBalanceTopUp {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// WVARA value to top up.
        #[arg()]
        value: u128,
        /// Flag to first approve given value on WVARA ERC20 contract.
        #[arg(short, long, default_value = "false")]
        approve: bool,
    },
    /// Send message to mirror program on Ethereum.
    SendMessage {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// Message payload.
        #[arg()]
        payload: Bytes,
        /// ETH value to send with message.
        #[arg()]
        value: u128,
        /// Flag to force mirror to make call to destination actor id on reply. If false, reply will be saved as logs.
        #[arg(short, long, default_value = "false")]
        call_reply: bool,
        /// Flag to watch for reply from mirror. If false, command will do not wait for reply.
        #[arg(short, long, default_value = "false")]
        watch: bool,
    },
}
