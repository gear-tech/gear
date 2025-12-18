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

#![allow(clippy::redundant_closure_call)]

use crate::{
    params::Params,
    utils::{
        Ethereum as EthereumCurrency, FormattedValue, RawOrFormattedValue,
        WrappedVara as WrappedVaraCurrency,
    },
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Parser, Subcommand};
use ethexe_common::Address;
use ethexe_ethereum::{Ethereum, mirror::ReplyInfo};
use ethexe_rpc::ProgramClient;
use ethexe_signer::Signer;
use gprimitives::{CodeId, H160, H256, MessageId, U256};
use jsonrpsee::ws_client::WsClientBuilder;
use serde::Serialize;
use serde_json::json;
use sp_core::Bytes;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
struct MirrorState {
    router: Address,
    state_hash: H256,
    nonce: U256,
    exited: bool,
    inheritor: Address,
    initializer: Address,
    balance: u128,
    formatted_balance: String,
    executable_balance: u128,
    formatted_executable_balance: String,
}

#[derive(Debug, Clone, Serialize)]
struct TopUpResult {
    tx_hash: H256,
    actor_id: Address,
    value: u128,
    formatted_value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum SendMessageResult {
    Simple {
        tx_hash: H256,
        message_id: MessageId,
    },
    WithReply {
        tx_hash: H256,
        reply_info: ReplyInfo,
    },
}

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
                json,
            } => {
                let upload_result = (async || -> Result<(H256, CodeId)> {
                    let code =
                        fs::read(&path_to_wasm).with_context(|| "failed to read wasm from file")?;

                    eprintln!("Uploading {} to Ethereum", path_to_wasm.display());

                    let pending_builder = router
                        .request_code_validation_with_sidecar(&code)
                        .await
                        .with_context(|| "failed to create code validation request")?;

                    let (tx, code_id) = pending_builder
                        .send()
                        .await
                        .with_context(|| "failed to request code validation")?;

                    eprintln!("Completed in transaction {tx:?}");

                    if watch {
                        eprintln!("Waiting for approval of code id {code_id}...");

                        let valid = router
                            .wait_code_validation(code_id)
                            .await
                            .with_context(|| "failed to wait for code validation")?;

                        if valid {
                            eprintln!("Now you can create program from code id {code_id}!");
                        } else {
                            bail!("Given code is invalid and failed validation");
                        }
                    }

                    Ok((tx, code_id))
                })()
                .await;

                if json {
                    let value = match &upload_result {
                        Ok((tx, code_id)) => json!({"tx_hash": tx, "code_id": code_id}),
                        Err(err) => json!({"error": format!("{err}")}),
                    };
                    println!("{value}");
                }

                upload_result?;
            }
            TxSubcommand::Create {
                code_id,
                salt,
                initializer,
                json,
            } => {
                let create_result = (async || -> Result<(H256, H160)> {
                    let salt = salt.unwrap_or_else(H256::random);
                    let override_initializer = initializer.map(Into::into);

                    eprintln!(
                        "Creating program on Ethereum from code id {code_id} and salt {salt:?}"
                    );

                    let (tx, actor_id) = router
                        .create_program(code_id, salt, override_initializer)
                        .await
                        .with_context(|| "failed to create program")?;

                    eprintln!("Completed in transaction {tx:?}");
                    eprintln!(
                        "Program address on Ethereum {:?}",
                        actor_id.to_address_lossy()
                    );

                    Ok((tx, actor_id.to_address_lossy()))
                })()
                .await;

                if json {
                    let value = match &create_result {
                        Ok((tx, actor_id)) => json!({"tx_hash": tx, "actor_id": actor_id}),
                        Err(err) => json!({"error": format!("{err}")}),
                    };
                    println!("{value}");
                }

                create_result?;
            }
            TxSubcommand::CreateWithAbi {
                code_id,
                salt,
                initializer,
                abi_interface,
                json,
            } => {
                let create_abi_result = (async || -> Result<(H256, H160)> {
                    let salt = salt.unwrap_or_else(H256::random);
                    let override_initializer = initializer.map(Into::into);

                    eprintln!(
                        "Creating program on Ethereum from code id {code_id} and salt {salt:?}"
                    );

                    let (tx, actor_id) = router
                        .create_program_with_abi_interface(
                            code_id,
                            salt,
                            override_initializer,
                            abi_interface.into(),
                        )
                        .await
                        .with_context(|| "failed to create program")?;

                    eprintln!("Completed in transaction {tx:?}");
                    eprintln!(
                        "Program address on Ethereum {:?}",
                        actor_id.to_address_lossy()
                    );

                    Ok((tx, actor_id.to_address_lossy()))
                })()
                .await;

                if json {
                    let value = match &create_abi_result {
                        Ok((tx, actor_id)) => json!({"tx_hash": tx, "actor_id": actor_id}),
                        Err(err) => json!({"error": format!("{err}")}),
                    };
                    println!("{value}");
                }

                create_abi_result?;
            }
            TxSubcommand::Query {
                rpc_url,
                mirror,
                json,
            } => {
                // TODO: consider moving this out of tx subcommand
                let query_result = (async || -> Result<MirrorState> {
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    eprintln!("Querying state of mirror on Ethereum at {mirror}");

                    let mirror = ethereum.mirror(mirror);
                    let mirror_query = mirror.query();

                    // TODO: consider crate like gsdk but for Vara.eth to avoid direct RPC calls
                    let ws_client: jsonrpsee::ws_client::WsClient = WsClientBuilder::new()
                        .build(&rpc_url)
                        .await
                        .with_context(|| "failed to create ws client for Vara.eth RPC")?;

                    let state_hash = mirror_query.state_hash().await?;
                    let program_state = ws_client.read_state(state_hash).await?;

                    let balance = program_state.balance;
                    let executable_balance = program_state.executable_balance;

                    let mirror_state = MirrorState {
                        router: mirror_query.router().await?,
                        state_hash,
                        nonce: mirror_query.nonce().await?,
                        exited: mirror_query.exited().await?,
                        inheritor: mirror_query.inheritor().await?,
                        initializer: mirror_query.initializer().await?,
                        balance,
                        formatted_balance: FormattedValue::<EthereumCurrency>::new(balance)
                            .to_string(),
                        executable_balance,
                        formatted_executable_balance: FormattedValue::<WrappedVaraCurrency>::new(
                            executable_balance,
                        )
                        .to_string(),
                    };

                    Ok(mirror_state)
                })()
                .await;

                if json {
                    let value = match &query_result {
                        Ok(mirror_state) => serde_json::to_string(mirror_state)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                let MirrorState {
                    router,
                    state_hash,
                    nonce,
                    exited,
                    inheritor,
                    initializer,
                    balance,
                    formatted_balance,
                    executable_balance,
                    formatted_executable_balance,
                } = query_result?;

                eprintln!("Mirror state:");
                eprintln!("  Router:          {router}");
                eprintln!("  State hash:      {state_hash:?}");
                eprintln!("  Nonce:           {nonce}");
                eprintln!("  Exited:          {exited}");
                eprintln!("  Inheritor:       {inheritor}",);
                eprintln!("  Initializer:     {initializer}",);
                eprintln!("  ETH Balance:     {balance} wei");
                eprintln!("  ETH Balance:     {formatted_balance}");
                eprintln!("  WVARA Balance:   {executable_balance}");
                eprintln!("  WVARA Balance:   {formatted_executable_balance}");
            }
            TxSubcommand::OwnedBalanceTopUp {
                mirror,
                value,
                watch,
                json,
            } => {
                let owned_balance_top_up_result = (async || -> Result<TopUpResult> {
                    let raw_value = value.into_inner();
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    let formatted_value = FormattedValue::<EthereumCurrency>::new(raw_value);
                    eprintln!(
                        "Topping up owned balance of mirror on Ethereum at {mirror} by {formatted_value} ({raw_value} wei)"
                    );

                    let mirror = ethereum.mirror(mirror);

                    let tx = mirror
                        .owned_balance_top_up(raw_value)
                        .await
                        .with_context(|| "failed to top up owned balance of mirror")?;

                    eprintln!("Completed in transaction {tx:?}");

                    if watch {
                        eprintln!("Waiting for state change...");

                        mirror
                            .wait_for_state_changed()
                            .await
                            .with_context(|| "failed to wait for state change")?;

                        eprintln!("Mirror state changed!");
                    }

                    eprintln!("Owned balance of mirror successfully topped up");

                    Ok(TopUpResult {
                        tx_hash: tx,
                        actor_id: mirror.address(),
                        value: raw_value,
                        formatted_value: formatted_value.to_string(),
                    })
                })()
                .await;

                if json {
                    let value = match &owned_balance_top_up_result {
                        Ok(top_up_result) => serde_json::to_string(top_up_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                owned_balance_top_up_result?;
            }
            TxSubcommand::ExecutableBalanceTopUp {
                mirror,
                value,
                approve,
                watch,
                json,
            } => {
                let executable_balance_top_up_result = (async || -> Result<TopUpResult> {
                    let raw_value = value.into_inner();
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    let formatted_value = FormattedValue::<WrappedVaraCurrency>::new(raw_value);
                    eprintln!(
                        "Topping up executable balance of mirror on Ethereum at {mirror} by {formatted_value} ({raw_value})"
                    );

                    let mirror = ethereum.mirror(mirror);

                    if raw_value != 0 && approve {
                        ethereum
                            .router()
                            .wvara()
                            .approve(mirror.address().into(), raw_value)
                            .await?;
                    }

                    let tx = mirror
                        .executable_balance_top_up(raw_value)
                        .await
                        .with_context(|| "failed to top up executable balance of mirror")?;

                    eprintln!("Completed in transaction {tx:?}");

                    if watch {
                        eprintln!("Waiting for state change...");

                        mirror
                            .wait_for_state_changed()
                            .await
                            .with_context(|| "failed to wait for state change")?;

                        eprintln!("Mirror state changed!");
                    }

                    eprintln!("Executable balance of mirror successfully topped up");

                    Ok(TopUpResult {
                        tx_hash: tx,
                        actor_id: mirror.address(),
                        value: raw_value,
                        formatted_value: formatted_value.to_string(),
                    })
                })()
                .await;

                if json {
                    let value = match &executable_balance_top_up_result {
                        Ok(top_up_result) => serde_json::to_string(top_up_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                executable_balance_top_up_result?;
            }
            TxSubcommand::SendMessage {
                mirror,
                payload,
                value,
                call_reply,
                watch,
                json,
            } => {
                let send_message_result = (async || -> Result<SendMessageResult> {
                    let raw_value = value.into_inner();
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    eprintln!("Sending message on Ethereum to {mirror}");

                    let mirror = ethereum.mirror(mirror);

                    let (tx, message_id) = mirror
                        .send_message(payload.0, raw_value, call_reply)
                        .await
                        .with_context(|| "failed to send message to mirror")?;

                    eprintln!("Completed in transaction {tx:?}");
                    eprintln!("Message with id {message_id} successfully sent");

                    Ok(if watch {
                        eprintln!("Waiting for reply...");

                        let reply_info = mirror.wait_for_reply(message_id).await?;
                        let ReplyInfo {
                            message_id,
                            actor_id,
                            payload,
                            code,
                            value,
                        } = &reply_info;

                        let actor_id = actor_id.to_address_lossy();
                        let raw_value = *value;
                        let formatted_value = FormattedValue::<EthereumCurrency>::new(raw_value);

                        eprintln!("Reply info:");
                        eprintln!("  Message Id: {message_id}");
                        eprintln!("  Actor Id:   {actor_id:?}");
                        eprintln!("  Payload:    0x{}", hex::encode(payload));
                        eprintln!("  Code:       {code:?}");
                        eprintln!("  Value:      {formatted_value} ({raw_value} wei)");

                        SendMessageResult::WithReply {
                            tx_hash: tx,
                            reply_info,
                        }
                    } else {
                        SendMessageResult::Simple {
                            tx_hash: tx,
                            message_id,
                        }
                    })
                })()
                .await;

                if json {
                    let value = match &send_message_result {
                        Ok(send_message_result) => serde_json::to_string(send_message_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                send_message_result?;
            }
        }

        Ok(())
    }
}

// TODO (breathx): impl reply, value claim.
// TODO (breathx) submit offchain txs
// TODO: consider --pending flag for some commands to just output pending tx hash
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
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
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
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
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
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Query mirror state on Vara.eth.
    Query {
        /// RPC URL of Vara.eth node. Example: ws://127.0.0.1:9944.
        #[arg(short, long)]
        rpc_url: String,
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Top up owned balance of mirror on Ethereum.
    OwnedBalanceTopUp {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// ETH value to top up.
        #[arg()]
        value: RawOrFormattedValue<EthereumCurrency>,
        /// Flag to watch for mirror state change. If false, command will do not wait mirror state change.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Top up executable balance of mirror on Ethereum.
    ExecutableBalanceTopUp {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// WVARA value to top up.
        #[arg()]
        value: RawOrFormattedValue<WrappedVaraCurrency>,
        /// Flag to first approve given value on WVARA ERC20 contract.
        #[arg(short, long, default_value = "false")]
        approve: bool,
        /// Flag to watch for mirror state change. If false, command will do not wait mirror state change.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
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
        value: RawOrFormattedValue<EthereumCurrency>,
        /// Flag to force mirror to make call to destination actor id on reply. If false, reply will be saved as logs.
        #[arg(short, long, default_value = "false", conflicts_with = "watch")]
        call_reply: bool,
        /// Flag to watch for reply from mirror. If false, command will do not wait for reply.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
}
