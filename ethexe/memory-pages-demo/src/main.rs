// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Demonstrates reading a deployed Vara.ETH program's memory pages.
//!
//! The demo:
//! 1. Spawns a local `ethexe run --dev` node.
//! 2. Uploads a tiny WAT program that writes recognizable bytes to memory during init.
//! 3. Creates the program and sends an empty handle message to drive a transition.
//! 4. Reads the written bytes back through the SDK's `MirrorMemory` APIs.

use anyhow::{Context, Result, ensure};
use ethexe_common::gear_core::pages::GearPage;
use ethexe_sdk::{VaraEthApi, node_bindings::VaraEth};
use gprimitives::H256;
use gsigner::secp256k1::{Address as EthereumAddress, Signer as Secp256k1Signer, PrivateKey};
use std::{str::FromStr, time::Duration};

/// Anvil sender account #2 used by `ethexe run --dev`.
///
/// The dev environment mints WVARA to sender accounts (accounts >= 2), while the
/// deployer account only receives ETH. We need WVARA to fund the program's
/// executable balance.
const SENDER_PRIVATE_KEY: &str =
    "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a";

/// Executable balance to give the demo program.
const PROGRAM_BALANCE: u128 = 500_000_000_000_000;

/// Simple WAT program that writes `0xDE 0xAD 0xBE 0xEF` at WASM offset `0x1000`
/// during `init`.
const WAT: &str = r#"
(module
    (import "env" "memory" (memory 32768))
    (export "init" (func $init))
    (func $init
        (i32.store8 (i32.const 0x1000) (i32.const 0xDE))
        (i32.store8 (i32.const 0x1001) (i32.const 0xAD))
        (i32.store8 (i32.const 0x1002) (i32.const 0xBE))
        (i32.store8 (i32.const 0x1003) (i32.const 0xEF))
    )
)
"#;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Spawning local Vara.ETH dev node...");
    let node = VaraEth::new()
        .timeout(Duration::from_secs(60))
        .spawn_ready()
        .await
        .context("failed to spawn dev node")?;

    let router_address = node
        .router_address()
        .await
        .context("failed to query router address")?;
    let vara_eth_rpc_url = node.ws_endpoint();
    let ethereum_rpc_url = node.ethereum_ws_endpoint();

    println!("  Router address: {router_address}");
    println!("  Vara.ETH RPC:   {vara_eth_rpc_url}");
    println!("  Ethereum RPC:   {ethereum_rpc_url}");

    let (signer, sender_address) = sender_signer()?;

    println!("Building SDK client...");
    let api = VaraEthApi::builder()
        .vara_eth_rpc_url(vara_eth_rpc_url)
        .ethereum_rpc_url(ethereum_rpc_url)
        .router_address(router_address)
        .signer(signer)
        .sender_address(sender_address)
        .build()
        .await
        .context("failed to build VaraEthApi")?;

    let wasm_binary = wat::parse_str(WAT).context("failed to parse WAT module")?;

    println!("Uploading WAT code...");
    let (_tx_hash, code_id) = api
        .router()
        .request_code_validation(&wasm_binary)
        .await
        .context("failed to request code validation")?;
    println!("  Code id: {code_id}");

    let validation = api
        .router()
        .wait_for_code_validation(code_id)
        .await
        .context("failed waiting for code validation")?;
    ensure!(
        validation.valid,
        "code validation failed: {validation:?}"
    );
    println!("  Code validated.");

    println!("Creating program...");
    let salt = H256::random();
    let (_tx_hash, program_id) = api
        .router()
        .create_program_with_executable_balance(code_id, salt, None, PROGRAM_BALANCE)
        .await
        .context("failed to create program")?;
    println!("  Program id: {program_id}");

    let mirror = api.mirror(program_id);

    println!("Sending empty handle message to drive a transition...");
    let (_tx_hash, message_id) = mirror
        .send_message(&[], 0)
        .await
        .context("failed to send message")?;
    println!("  Message id: {message_id}");

    let reply = mirror
        .wait_for_reply(message_id)
        .await
        .context("failed waiting for reply")?;
    println!("  Reply code: {:?}", reply.code);

    println!("Reading program memory...");
    let memory = mirror
        .memory()
        .await
        .context("failed to read memory handle")?
        .context("program has no active memory")?;

    let page = GearPage::from_offset(0x1000);
    let page_data = memory
        .page(page)
        .await
        .context("failed to read memory page")?;

    match page_data {
        Some(data) => {
            println!("  Page {} loaded ({} bytes)", page, data.len());
            let offset_in_page = (0x1000 - page.offset()) as usize;
            let bytes = &data[offset_in_page..offset_in_page + 4];
            println!("  Bytes at 0x1000: {:02x?}", bytes);
            ensure!(
                bytes == [0xDE, 0xAD, 0xBE, 0xEF],
                "unexpected bytes in memory: {:02x?}",
                bytes
            );
        }
        None => anyhow::bail!("target page is not materialized in storage"),
    }

    for i in 0..4 {
        let byte = memory
            .byte(0x1000 + i)
            .await
            .with_context(|| format!("failed to read byte at 0x{:x}", 0x1000 + i))?
            .with_context(|| format!("byte at 0x{:x} is not materialized", 0x1000 + i))?;
        println!("  byte(0x{:x}) = 0x{:02x}", 0x1000 + i, byte);
    }

    println!("Demo succeeded: memory bytes match 0xDE 0xAD 0xBE 0xEF");
    Ok(())
}

fn sender_signer() -> Result<(Secp256k1Signer, EthereumAddress)> {
    let private_key = PrivateKey::from_str(SENDER_PRIVATE_KEY.trim_start_matches("0x"))?;
    let signer = Secp256k1Signer::memory();
    let public_key = signer.import(private_key)?;
    let address = public_key.to_address();

    Ok((signer, address))
}
