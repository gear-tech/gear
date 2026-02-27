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
use alloy_chains::NamedChain;
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Parser, Subcommand};
use ethexe_common::{
    Address, BlockHeader, SimpleBlockData,
    gear_core::{ids::prelude::CodeIdExt, limited::LimitedVec, rpc::ReplyInfo},
    injected::{AddressedInjectedTransaction, InjectedTransaction},
};
use ethexe_ethereum::{Ethereum, mirror::ClaimInfo, router::CodeValidationResult};
use ethexe_rpc::{InjectedClient, ProgramClient};
use gprimitives::{ActorId, CodeId, H160, H256, MessageId, U256};
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use jsonrpsee::ws_client::WsClientBuilder;
use serde::Serialize;
use serde_json::json;
use sp_core::Bytes;
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
struct UploadResultData {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,
    blob_gas_used: Option<u64>,
    blob_gas_price: Option<u128>,
    blob_fee_wei: Option<U256>,

    code_id: CodeId,
    code_size_bytes: usize,
    code_validation_result: Option<CodeValidationResult>,
}

#[derive(Debug, Clone, Serialize)]
struct CreateResultData {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,

    actor_id: H160,
    salt: H256,
    initializer: ActorId,
    abi_interface: Option<Address>,
}

#[derive(Debug, Clone, Serialize)]
struct MirrorState {
    router: Address,
    state_hash: H256,
    nonce: U256,
    exited: bool,
    inheritor: ActorId,
    initializer: ActorId,
    balance: u128,
    formatted_balance: String,
    executable_balance: u128,
    formatted_executable_balance: String,
}

#[derive(Debug, Clone, Serialize)]
struct TopUpResult {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,

    actor_id: H160,
    value: u128,
    formatted_value: String,
}

#[derive(Debug, Clone, Serialize)]
struct SendMessagePayload {
    message_id: MessageId,
    actor_id: H160,
    payload_len: usize,
    payload_hex: String,
    raw_value: u128,
    formatted_value: String,
    reply_info: Option<ReplyInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum SendMessageResult {
    Ethereum {
        chain_id: u64,
        tx_hash: H256,
        explorer_url: Option<String>,
        block_number: Option<u64>,
        block_hash: Option<H256>,
        gas_used: u64,
        effective_gas_price: u128,
        total_fee_wei: U256,

        #[serde(flatten)]
        payload: SendMessagePayload,
    },
    Injected {
        tx_hash: H256,
        reference_block_number: u64,
        reference_block_hash: H256,

        #[serde(flatten)]
        payload: SendMessagePayload,
    },
}

#[derive(Debug, Clone, Serialize)]
struct SendReplyResult {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,

    actor_id: H160,
    replied_to: MessageId,
    payload_len: usize,
    payload_hex: String,
    raw_value: u128,
    formatted_value: String,
    claim_info: Option<ClaimInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct ClaimValueResult {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,

    actor_id: H160,
    claimed_id: MessageId,
    claim_info: Option<ClaimInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct TransferLockedValueToInheritorResult {
    chain_id: u64,
    tx_hash: H256,
    explorer_url: Option<String>,
    block_number: Option<u64>,
    block_hash: Option<H256>,
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,

    actor_id: H160,
    value: u128,
    formatted_value: String,
}

/// Submit a transaction.
#[derive(Debug, Parser)]
pub struct TxCommand {
    /// Primary key store to use (use to override generation from base path).
    #[arg(long)]
    pub key_store: Option<PathBuf>,
    /// Print additional details (long payloads, etc.).
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,

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
        let key_store = self
            .key_store
            .with_context(|| "must never be empty after merging")?;
        let _verbose = self.verbose;

        let signer = Signer::fs(key_store)?;

        let rpc = self
            .ethereum_rpc
            .ok_or_else(|| anyhow!("missing `ethereum-rpc`"))?;

        let router_addr = self
            .ethereum_router
            .ok_or_else(|| anyhow!("missing `ethereum-router`"))?;

        let sender = self.sender.ok_or_else(|| anyhow!("missing `sender`"))?;

        let ethereum = Ethereum::new(&rpc, router_addr, signer.clone(), sender)
            .await
            .with_context(|| "failed to create Ethereum client")?;

        eprintln!("RPC:      {rpc}");
        if let TxSubcommand::Query { rpc_url, .. }
        | TxSubcommand::SendMessage {
            rpc_url: Some(rpc_url),
            injected: true,
            ..
        } = &self.command
        {
            eprintln!("WS RPC:   {rpc_url}");
        }
        let router = ethereum.router();
        let router_query = router.query();
        let chain_id = ethereum
            .chain_id()
            .await
            .with_context(|| "failed to fetch chain id")?;

        eprintln!("Router:   {router_addr}");
        if let Some(url) = explorer_address_link(chain_id, router.address()) {
            eprintln!("Explorer: {url}");
        }
        eprintln!("Sender:   {sender}");
        if let Some(url) = explorer_address_link(chain_id, sender) {
            eprintln!("Explorer: {url}");
        }

        let chain_name = NamedChain::try_from(chain_id)
            .ok()
            .map(|named_chain| named_chain.as_str())
            .unwrap_or("unknown");
        eprintln!("Chain id: {chain_id} ({chain_name})");
        eprintln!();

        match self.command {
            TxSubcommand::Upload {
                path_to_wasm,
                watch,
                json,
            } => {
                let upload_result = (async || -> Result<UploadResultData> {
                    let code =
                        fs::read(&path_to_wasm).with_context(|| "failed to read wasm from file")?;
                    let code_id = CodeId::generate(&code);
                    let code_size_bytes = code.len();
                    let code_size_kib = code_size_bytes as f64 / 1024.0;

                    eprintln!("Uploading {} to Ethereum", path_to_wasm.display());
                    eprintln!("  Code id:   {code_id} (blake2b256)");
                    eprintln!("  Code size: {code_size_bytes} bytes ({code_size_kib:.2} KiB)",);

                    let (receipt, code_id) = router
                        .request_code_validation_with_receipt(&code)
                        .await
                        .with_context(|| {
                            format!("failed to create code validation request (code_id {code_id})")
                        })?;
                    let tx_hash = (*receipt.transaction_hash).into();
                    eprintln!();

                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();

                    let mut upload_result = UploadResultData {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        blob_gas_used: fee.blob_gas_used,
                        blob_gas_price: fee.blob_gas_price,
                        blob_fee_wei: fee.blob_fee_wei,
                        code_id,
                        code_size_bytes,
                        code_validation_result: None,
                    };

                    if watch {
                        eprintln!();
                        eprintln!("Waiting for approval of code (`--watch` option was passed)...");
                        eprintln!();

                        let code_validation_result = router
                            .wait_for_code_validation(code_id)
                            .await
                            .with_context(|| "failed to wait for code validation")?;

                        if code_validation_result.valid {
                            eprintln!("Code validation request approved:");
                            if let Some(tx_hash) = code_validation_result.tx_hash {
                                eprintln!("  Tx hash:      {tx_hash:?}");
                                let explorer_url = explorer_link(chain_id, tx_hash);
                                if let Some(url) = &explorer_url {
                                    eprintln!("  Explorer:     {url}");
                                }
                            }
                            if let Some(block_number) = code_validation_result.block_number {
                                eprintln!("  Block number: {block_number}");
                            }
                            if let Some(block_hash) = code_validation_result.block_hash {
                                eprintln!("  Block hash:   {block_hash:?}");
                            }
                            eprintln!();

                            let command_name =
                                env::args().next().unwrap_or_else(|| "ethexe".into());

                            eprintln!("Now you can create program from code id:");
                            eprintln!("  Code id: {code_id}");
                            eprintln!(
                                "  Command: {command_name} tx --sender {sender} create {code_id}"
                            );
                        } else {
                            bail!("Given code is invalid and failed validation");
                        }

                        upload_result.code_validation_result = Some(code_validation_result);
                    }

                    Ok(upload_result)
                })()
                .await;

                if json {
                    let value = match &upload_result {
                        Ok(upload_result) => serde_json::to_string(upload_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
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
                let create_result = (async || -> Result<CreateResultData> {
                    let salt = salt.unwrap_or_else(H256::random);
                    let override_initializer = initializer.map(Into::into);
                    let initializer = initializer.unwrap_or(sender);

                    eprintln!("Creating program on Ethereum from code id, salt and initializer:");
                    eprintln!("  Code id:     {code_id}");
                    eprintln!("  Salt:        {salt:?}");
                    eprintln!("  Initializer: {initializer}");
                    eprintln!();

                    let (receipt, actor_id) = router
                        .create_program_with_receipt(code_id, salt, override_initializer)
                        .await
                        .with_context(|| {
                            format!(
                                "failed to create program from code id {code_id} and salt {salt:?}"
                            )
                        })?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    let actor_id = actor_id.to_address_lossy();
                    eprintln!("Program created from code id, salt and initializer:");
                    eprintln!("  Actor id: {actor_id:?}");
                    if let Some(url) = explorer_address_link(chain_id, actor_id.into()) {
                        eprintln!("  Explorer: {url}");
                    }

                    Ok(CreateResultData {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
                        salt,
                        initializer: initializer.into(),
                        abi_interface: None,
                    })
                })()
                .await;

                if json {
                    let value = match &create_result {
                        Ok(create_result) => serde_json::to_string(create_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
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
                let create_abi_result = (async || -> Result<CreateResultData> {
                    let salt = salt.unwrap_or_else(H256::random);
                    let override_initializer = initializer.map(Into::into);
                    let initializer = initializer.unwrap_or(sender);

                    eprintln!("Creating program with ABI interface on Ethereum from code id, salt and initializer:");
                    eprintln!("  Code id:       {code_id}");
                    eprintln!("  Salt:          {salt:?}");
                    eprintln!("  Initializer:   {initializer}");
                    eprintln!("  ABI interface: {abi_interface}");
                    eprintln!();

                    let (receipt, actor_id) = router
                        .create_program_with_abi_interface_with_receipt(
                            code_id,
                            salt,
                            override_initializer,
                            abi_interface.into(),
                        )
                        .await
                        .with_context(|| {
                            format!(
                                "failed to create program with ABI interface from code id {code_id} and salt {salt:?}"
                            )
                        })?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    let actor_id = actor_id.to_address_lossy();
                    eprintln!("Program with ABI interface created from code id, salt and initializer:");
                    eprintln!("  Actor id: {actor_id:?}");
                    if let Some(url) = explorer_address_link(chain_id, actor_id.into()) {
                        eprintln!("  Explorer: {url}");
                    }

                    Ok(CreateResultData {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
                        salt,
                        initializer: initializer.into(),
                        abi_interface: Some(abi_interface),
                    })
                })()
                .await;

                if json {
                    let value = match &create_abi_result {
                        Ok(create_abi_result) => serde_json::to_string(create_abi_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
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

                    eprintln!("Querying state of mirror on Ethereum");
                    eprintln!("  Mirror: {mirror}");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
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

                if let Ok(MirrorState {
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
                }) = &query_result
                {
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

                if json {
                    let value = match &query_result {
                        Ok(mirror_state) => serde_json::to_string(mirror_state)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }
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
                    eprintln!("Topping up owned balance of mirror on Ethereum:");
                    eprintln!("  Mirror: {mirror}");
                    eprintln!("  Value:  {formatted_value} ({raw_value} wei)");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
                    let actor_id: ActorId = mirror.actor_id();
                    let actor_id = actor_id.to_address_lossy();

                    let receipt = mirror
                        .owned_balance_top_up_with_receipt(raw_value)
                        .await
                        .with_context(|| "failed to top up owned balance of mirror")?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    if watch {
                        eprintln!("Waiting for state change...");

                        mirror
                            .wait_for_state_change()
                            .await
                            .with_context(|| "failed to wait for state change")?;

                        eprintln!("Mirror state changed!");
                    }

                    eprintln!("Owned balance of mirror successfully topped up!");

                    Ok(TopUpResult {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
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
                    eprintln!("Topping up executable balance of mirror on Ethereum:");
                    eprintln!("  Mirror: {mirror}");
                    eprintln!("  Value:  {formatted_value} ({raw_value})");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
                    let actor_id: ActorId = mirror.actor_id();
                    let actor_id = actor_id.to_address_lossy();

                    // TODO: consider to get receipt from approve tx as well
                    if raw_value != 0 && approve {
                        ethereum
                            .router()
                            .wvara()
                            .approve(mirror.actor_id(), raw_value)
                            .await?;
                    }

                    let receipt = mirror
                        .executable_balance_top_up_with_receipt(raw_value)
                        .await
                        .with_context(|| "failed to top up executable balance of mirror")?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    if watch {
                        eprintln!("Waiting for state change...");

                        mirror
                            .wait_for_state_change()
                            .await
                            .with_context(|| "failed to wait for state change")?;

                        eprintln!("Mirror state changed!");
                    }

                    eprintln!("Executable balance of mirror successfully topped up!");

                    Ok(TopUpResult {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
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
                rpc_url,
                injected,
                watch,
                json,
            } => {
                let send_message_result = (async || -> Result<SendMessageResult> {
                    let raw_value = value.into_inner();
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    if rpc_url.is_some() && injected && raw_value != 0 {
                        // TODO: consider allowing this in future
                        bail!("Cannot send value along with injected message");
                    }

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    let payload_len = payload.0.len();
                    // TODO: consider truncating long payloads in non-verbose mode and hexdump in verbose mode
                    let payload_hex = format!("0x{}", hex::encode(&payload.0));
                    let formatted_value = FormattedValue::<EthereumCurrency>::new(raw_value);
                    if rpc_url.is_some() && injected {
                        eprintln!("Sending injected message to program:");
                    } else {
                        eprintln!("Sending message to program on Ethereum:");
                    }
                    eprintln!("  Mirror:      {mirror}");
                    eprintln!("  Payload len: {payload_len} bytes");
                    eprintln!("  Payload hex: {payload_hex}");
                    eprintln!("  Value:       {formatted_value} ({raw_value} wei)");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
                    let raw_actor_id: ActorId = mirror.actor_id();
                    let actor_id = raw_actor_id.to_address_lossy();

                    if let Some(rpc_url) = &rpc_url
                        && injected
                    {
                        // TODO: consider crate like gsdk but for Vara.eth to avoid direct RPC calls
                        let ws_client: jsonrpsee::ws_client::WsClient = WsClientBuilder::new()
                            .build(rpc_url)
                            .await
                            .with_context(|| "failed to create ws client for Vara.eth RPC")?;

                        let public_key = signer
                            .get_key_by_address(sender)
                            .with_context(|| format!("failed to get key for sender {sender}"))?
                            .ok_or_else(|| anyhow!("no key found for {sender}"))?;

                        //let (reference_block_number, reference_block_hash) =
                        let SimpleBlockData {
                            hash: reference_block_hash,
                            header:
                                BlockHeader {
                                    height: reference_block_number,
                                    ..
                                },
                        } = ethereum.get_latest_block().await?;
                        let reference_block_number = reference_block_number as u64;
                        let salt = H256::random();

                        let injected_transaction = InjectedTransaction {
                            destination: raw_actor_id,
                            payload: payload.0.clone().try_into().unwrap(),
                            value: raw_value,
                            reference_block: reference_block_hash,
                            salt: LimitedVec::try_from(salt.as_bytes())
                                .expect("`H256` is small enough for a salt"),
                        };
                        let message_id = injected_transaction.to_message_id();
                        let tx_hash = injected_transaction.to_hash().into();

                        let transaction = AddressedInjectedTransaction {
                            recipient: Address::default(),
                            tx: signer
                                .signed_message(public_key, injected_transaction, None)
                                .with_context(|| "failed to create signed injected transaction")?,
                        };

                        if !watch {
                            ws_client
                                .send_transaction(transaction.clone())
                                .await
                                .with_context(|| "failed to send injected transaction")?;
                        }

                        // TODO: consider adding tx fee estimation in ETH here?
                        eprintln!("Completed, injected transaction info:");
                        eprintln!("  Tx hash:      {tx_hash:?}");
                        eprintln!("  Block number: {reference_block_number:<66} (reference block)");
                        eprintln!("  Block hash:   {reference_block_hash:?} (reference block)");
                        eprintln!("  Salt:         {salt:?}");
                        eprintln!();

                        eprintln!("Message successfully sent:");
                        eprintln!("  Message id: {message_id:?}");
                        eprintln!();

                        let reply_info = if watch {
                            eprintln!("Waiting for reply (promise for injected transaction)...");

                            let mut subscription = ws_client
                                .send_transaction_and_watch(transaction)
                                .await
                                .with_context(
                                    || "failed to send injected transaction to Vara.eth RPC",
                                )?;

                            let promise = subscription
                                .next()
                                .await
                                .ok_or_else(|| anyhow!("no promise received from subscription"))?
                                .with_context(|| "failed to receive transaction promise")?
                                .into_data();
                            let ReplyInfo {
                                payload,
                                value,
                                code,
                            } = promise.reply;

                            let payload_len = payload.len();
                            // TODO: consider truncating long payloads in non-verbose mode and hexdump in verbose mode
                            let payload_hex = format!("0x{}", hex::encode(&payload));
                            let code_hex = format!("0x{}", hex::encode(code.to_bytes()));
                            let raw_value = value;
                            let formatted_value =
                                FormattedValue::<EthereumCurrency>::new(raw_value);

                            eprintln!("Reply info:");
                            eprintln!("  Message id:  {message_id}");
                            eprintln!("  Actor id:    {actor_id:?}");
                            eprintln!("  Payload len: {payload_len} bytes");
                            eprintln!("  Payload hex: {payload_hex}");
                            eprintln!("  Code:        {code:?} ({code_hex})");
                            eprintln!("  Value:       {formatted_value} ({raw_value} wei)");

                            Some(ReplyInfo {
                                payload,
                                value: raw_value,
                                code,
                            })
                        } else {
                            eprintln!(
                                "To wait for the reply, run this command with `--watch` flag"
                            );
                            None
                        };

                        Ok(SendMessageResult::Injected {
                            tx_hash,
                            reference_block_number,
                            reference_block_hash,
                            payload: SendMessagePayload {
                                message_id,
                                actor_id,
                                payload_len,
                                payload_hex,
                                raw_value,
                                formatted_value: formatted_value.to_string(),
                                reply_info,
                            },
                        })
                    } else {
                        let (receipt, message_id) = mirror
                            .send_message_with_receipt(payload.0.clone(), raw_value)
                            .await
                            .with_context(|| {
                                format!("failed to send message to mirror {actor_id}")
                            })?;

                        let tx_hash = (*receipt.transaction_hash).into();
                        let fee = TxCostSummary::new(
                            receipt.gas_used,
                            receipt.effective_gas_price,
                            receipt.blob_gas_used,
                            receipt.blob_gas_price,
                        );
                        let block_number = receipt.block_number;
                        let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                        eprintln!("Completed, transaction receipt:");
                        eprintln!("  Tx hash:      {tx_hash:?}");
                        let explorer_url = explorer_link(chain_id, tx_hash);
                        if let Some(url) = &explorer_url {
                            eprintln!("  Explorer:     {url}");
                        }
                        if let Some(block_number) = block_number {
                            eprintln!("  Block number: {block_number}");
                        }
                        if let Some(block_hash) = block_hash {
                            eprintln!("  Block hash:   {block_hash:?}");
                        }
                        fee.print_human();
                        eprintln!();

                        eprintln!("Message successfully sent:");
                        eprintln!("  Message id: {message_id:?}");
                        eprintln!();

                        let reply_info = if watch {
                            eprintln!("Waiting for reply...");

                            let reply_info = mirror.wait_for_reply(message_id).await?;
                            let ReplyInfo {
                                payload,
                                value,
                                code,
                            } = &reply_info;

                            let payload_len = payload.len();
                            // TODO: consider truncating long payloads in non-verbose mode and hexdump in verbose mode
                            let payload_hex = format!("0x{}", hex::encode(payload));
                            let code_hex = format!("0x{}", hex::encode(code.to_bytes()));
                            let raw_value = *value;
                            let formatted_value =
                                FormattedValue::<EthereumCurrency>::new(raw_value);

                            eprintln!("Reply info:");
                            eprintln!("  Message id:  {message_id}");
                            eprintln!("  Actor id:    {actor_id:?}");
                            eprintln!("  Payload len: {payload_len} bytes");
                            eprintln!("  Payload hex: {payload_hex}");
                            eprintln!("  Code:        {code:?} ({code_hex})");
                            eprintln!("  Value:       {formatted_value} ({raw_value} wei)");

                            Some(reply_info)
                        } else {
                            eprintln!(
                                "To wait for the reply, run this command with `--watch` flag"
                            );
                            None
                        };

                        Ok(SendMessageResult::Ethereum {
                            chain_id,
                            tx_hash,
                            explorer_url,
                            block_number,
                            block_hash,
                            gas_used: fee.gas_used,
                            effective_gas_price: fee.effective_gas_price,
                            total_fee_wei: fee.total_fee_wei,
                            payload: SendMessagePayload {
                                message_id,
                                actor_id,
                                payload_len,
                                payload_hex,
                                raw_value,
                                formatted_value: formatted_value.to_string(),
                                reply_info,
                            },
                        })
                    }
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
            TxSubcommand::SendReply {
                mirror,
                replied_to,
                payload,
                value,
                watch,
                json,
            } => {
                let send_reply_result = (async || -> Result<SendReplyResult> {
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    let payload_len = payload.0.len();
                    // TODO: consider truncating long payloads in non-verbose mode and hexdump in verbose mode
                    let payload_hex = format!("0x{}", hex::encode(&payload.0));
                    let raw_value = value.into_inner();
                    let formatted_value = FormattedValue::<EthereumCurrency>::new(raw_value);

                    eprintln!("Sending reply to program on Ethereum:");
                    eprintln!("  Mirror:      {mirror}");
                    eprintln!("  Replied to:  {replied_to}");
                    eprintln!("  Payload len: {payload_len} bytes");
                    eprintln!("  Payload hex: {payload_hex}");
                    eprintln!("  Value:       {formatted_value} ({raw_value} wei)");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
                    let raw_actor_id: ActorId = mirror.actor_id();
                    let actor_id = raw_actor_id.to_address_lossy();

                    let (receipt, _) = mirror
                        .send_reply_with_receipt(replied_to, payload.0.clone(), raw_value)
                        .await
                        .with_context(|| format!("failed to send reply to mirror {actor_id:?}"))?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    eprintln!("Reply successfully sent!");

                    let claim_info = if watch {
                        eprintln!("Waiting for value to be claimed...");

                        let claim_info = mirror.wait_for_value_claim(replied_to).await?;
                        let ClaimInfo {
                            message_id,
                            actor_id,
                            value,
                        } = &claim_info;

                        let actor_id = actor_id.to_address_lossy();
                        let raw_value = *value;
                        let formatted_value =
                            FormattedValue::<EthereumCurrency>::new(raw_value);

                        eprintln!("Claim info:");
                        eprintln!("  Message id:  {message_id}");
                        eprintln!("  Actor id:    {actor_id:?}");
                        eprintln!("  Value:       {formatted_value} ({raw_value} wei)");

                        Some(claim_info)
                    } else {
                        eprintln!(
                            "To wait for the value to be claimed, run this command with `--watch` flag"
                        );
                        None
                    };

                    Ok(SendReplyResult {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
                        replied_to,
                        payload_len,
                        payload_hex,
                        raw_value,
                        formatted_value: formatted_value.to_string(),
                        claim_info,
                    })
                })()
                .await;

                if json {
                    let value = match &send_reply_result {
                        Ok(send_reply_result) => serde_json::to_string(send_reply_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                send_reply_result?;
            }
            TxSubcommand::ClaimValue {
                mirror,
                claimed_id,
                watch,
                json,
            } => {
                let claim_value_result = (async || -> Result<ClaimValueResult> {
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    eprintln!("Claiming value from program on Ethereum:");
                    eprintln!("  Mirror:     {mirror}");
                    eprintln!("  Claimed id: {claimed_id}");
                    eprintln!();

                    let mirror = ethereum.mirror(mirror.into());
                    let raw_actor_id: ActorId = mirror.actor_id();
                    let actor_id = raw_actor_id.to_address_lossy();

                    let receipt = mirror
                        .claim_value_with_receipt(claimed_id)
                        .await
                        .with_context(|| {
                            format!("failed to claim value from mirror {actor_id:?}")
                        })?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    eprintln!("Value claim successfully requested!");
                    eprintln!();

                    let claim_info = if watch {
                        eprintln!("Waiting for value to be claimed...");

                        let claim_info = mirror.wait_for_value_claim(claimed_id).await?;
                        let ClaimInfo {
                            message_id,
                            actor_id,
                            value,
                        } = &claim_info;

                        let actor_id = actor_id.to_address_lossy();
                        let raw_value = *value;
                        let formatted_value =
                            FormattedValue::<EthereumCurrency>::new(raw_value);

                        eprintln!("Claim info:");
                        eprintln!("  Message id:  {message_id}");
                        eprintln!("  Actor id:    {actor_id:?}");
                        eprintln!("  Value:       {formatted_value} ({raw_value} wei)");

                        Some(claim_info)
                    } else {
                        eprintln!(
                            "To wait for the value to be claimed, run this command with `--watch` flag"
                        );
                        None
                    };

                    Ok(ClaimValueResult {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
                        claimed_id,
                        claim_info,
                    })
                })()
                .await;

                if json {
                    let value = match &claim_value_result {
                        Ok(claim_value_result) => serde_json::to_string(claim_value_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                claim_value_result?;
            }
            TxSubcommand::TransferLockedValueToInheritor { mirror, json } => {
                let transfer_result = (async || -> Result<TransferLockedValueToInheritorResult> {
                    let maybe_code_id = router_query
                        .program_code_id(mirror.into())
                        .await
                        .with_context(|| "failed to check if mirror in known by router")?;

                    ensure!(
                        maybe_code_id.is_some(),
                        "Given mirror address is not recognized by router"
                    );

                    let mirror = ethereum.mirror(mirror.into());
                    let raw_actor_id: ActorId = mirror.actor_id();
                    let actor_id = raw_actor_id.to_address_lossy();
                    let value =
                        mirror.query().balance().await.with_context(|| {
                            format!("failed to get balance of mirror {actor_id:?}")
                        })?;
                    let formatted_value = FormattedValue::<EthereumCurrency>::new(value);

                    ensure!(
                        value > 0,
                        "Mirror {actor_id:?} has no locked value to transfer"
                    );

                    eprintln!("Transferring locked value from mirror to inheritor on Ethereum:");
                    eprintln!("  Mirror: {actor_id:?}");
                    eprintln!("  Value:  {formatted_value} ({value} wei)");
                    eprintln!();

                    let receipt = mirror
                        .transfer_locked_value_to_inheritor_with_receipt()
                        .await
                        .with_context(|| {
                            format!("failed to transfer locked value from mirror {actor_id:?}")
                        })?;

                    let tx_hash = (*receipt.transaction_hash).into();
                    let fee = TxCostSummary::new(
                        receipt.gas_used,
                        receipt.effective_gas_price,
                        receipt.blob_gas_used,
                        receipt.blob_gas_price,
                    );
                    let block_number = receipt.block_number;
                    let block_hash = receipt.block_hash.map(|block_hash| H256(block_hash.0));

                    eprintln!("Completed, transaction receipt:");
                    eprintln!("  Tx hash:      {tx_hash:?}");
                    let explorer_url = explorer_link(chain_id, tx_hash);
                    if let Some(url) = &explorer_url {
                        eprintln!("  Explorer:     {url}");
                    }
                    if let Some(block_number) = block_number {
                        eprintln!("  Block number: {block_number}");
                    }
                    if let Some(block_hash) = block_hash {
                        eprintln!("  Block hash:   {block_hash:?}");
                    }
                    fee.print_human();
                    eprintln!();

                    eprintln!("Locked value successfully transferred to inheritor!");

                    Ok(TransferLockedValueToInheritorResult {
                        chain_id,
                        tx_hash,
                        explorer_url,
                        block_number,
                        block_hash,
                        gas_used: fee.gas_used,
                        effective_gas_price: fee.effective_gas_price,
                        total_fee_wei: fee.total_fee_wei,
                        actor_id,
                        value,
                        formatted_value: formatted_value.to_string(),
                    })
                })()
                .await;

                if json {
                    let value = match &transfer_result {
                        Ok(transfer_result) => serde_json::to_string(transfer_result)?,
                        Err(err) => json!({"error": format!("{err}")}).to_string(),
                    };
                    println!("{value}");
                }

                transfer_result?;
            }
        }

        Ok(())
    }
}

fn explorer_link(chain_id: u64, tx_hash: H256) -> Option<String> {
    explorer_base(chain_id).map(|base| format!("{base}/tx/{tx_hash:?}"))
}

fn explorer_address_link(chain_id: u64, address: Address) -> Option<String> {
    explorer_base(chain_id).map(|base| format!("{base}/address/{address:?}"))
}

fn explorer_base(chain_id: u64) -> Option<&'static str> {
    let named_chain: NamedChain = chain_id.try_into().ok()?;
    named_chain.etherscan_urls().map(|(_, base_url)| base_url)
}

#[derive(Debug, Clone)]
struct TxCostSummary {
    gas_used: u64,
    effective_gas_price: u128,
    total_fee_wei: U256,
    blob_gas_used: Option<u64>,
    blob_gas_price: Option<u128>,
    blob_fee_wei: Option<U256>,
}

impl TxCostSummary {
    fn new(
        gas_used: u64,
        effective_gas_price: u128,
        blob_gas_used: Option<u64>,
        blob_gas_price: Option<u128>,
    ) -> Self {
        let total_fee_wei = U256::from(gas_used) * U256::from(effective_gas_price);
        let blob_fee_wei = blob_gas_used
            .zip(blob_gas_price)
            .map(|(used, price)| U256::from(used).saturating_mul(U256::from(price)));

        Self {
            gas_used,
            effective_gas_price,
            total_fee_wei,
            blob_gas_used,
            blob_gas_price,
            blob_fee_wei,
        }
    }

    fn print_human(&self) {
        let Self {
            gas_used,
            effective_gas_price,
            total_fee_wei,
            blob_gas_used,
            blob_gas_price,
            blob_fee_wei,
        } = *self;

        eprintln!("  Gas used:     {gas_used}");
        eprintln!("  Gas price:    {effective_gas_price} wei");

        let formatted_total_fee = if total_fee_wei <= U256::from(u128::MAX) {
            Some(FormattedValue::<EthereumCurrency>::new(total_fee_wei.low_u128()).to_string())
        } else {
            None
        };
        if let Some(formatted_total_fee) = formatted_total_fee {
            eprintln!("  Total fee:    {total_fee_wei} wei ({formatted_total_fee})");
        } else {
            eprintln!("  Total fee:    {total_fee_wei} wei");
        }

        if let Some((blob_used, blob_price, blob_fee)) = blob_gas_used
            .zip(blob_gas_price)
            .zip(blob_fee_wei)
            .map(|((used, price), fee)| (used, price, fee))
        {
            let formatted_blob_fee = if blob_fee <= U256::from(u128::MAX) {
                Some(FormattedValue::<EthereumCurrency>::new(blob_fee.low_u128()).to_string())
            } else {
                None
            };
            if let Some(formatted_blob_fee) = formatted_blob_fee {
                eprintln!(
                    "  Blob gas fee: {blob_fee} wei ({formatted_blob_fee}) on {blob_used} blob gas @ {blob_price} wei"
                );
            } else {
                eprintln!(
                    "  Blob gas fee: {blob_fee} wei on {blob_used} blob gas @ {blob_price} wei"
                );
            }
        }
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
        /// RPC URL of Vara.eth node. Example: ws://127.0.0.1:9944. Used only if `injected` is true.
        #[arg(short, long, requires = "injected")]
        rpc_url: Option<String>,
        /// Flag to send injected transaction. If false, normal transaction is sent.
        #[arg(short, long, default_value = "false", requires = "rpc-url")]
        injected: bool,
        /// Flag to watch for reply from mirror. If false, command will do not wait for reply.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Send reply to mirror program on Ethereum.
    SendReply {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// Message id to reply to.
        #[arg()]
        replied_to: MessageId,
        /// Reply payload.
        #[arg()]
        payload: Bytes,
        /// ETH value to send with reply.
        #[arg()]
        value: RawOrFormattedValue<EthereumCurrency>,
        /// Flag to watch for value claimed from mirror. If false, command will do not wait for value claimed.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Claim value from mirror program on Ethereum.
    ClaimValue {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// Message id to claim value for.
        #[arg()]
        claimed_id: MessageId,
        /// Flag to watch for value claimed from mirror. If false, command will do not wait for value claimed.
        #[arg(short, long, default_value = "false")]
        watch: bool,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    /// Transfer locked value from mirror to inheritor on Ethereum.
    TransferLockedValueToInheritor {
        /// Mirror address.
        #[arg()]
        mirror: Address,
        /// Flag to output result in JSON format. If false, human-readable format is used.
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
}
