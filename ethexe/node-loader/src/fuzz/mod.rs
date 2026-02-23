// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Fuzz mode for the ethexe-node-loader.
//!
//! Deploys a "mega" syscall-exercising contract once, then repeatedly sends
//! randomised [`FuzzCommand`](demo_syscalls_ethexe::FuzzCommand) sequences to it, verifying that each execution
//! either succeeds or traps in an expected way.

mod cmd_gen;

use crate::args::FuzzParams;
use alloy::primitives::Address;
use anyhow::Result;
use demo_syscalls_ethexe::InitConfig;
use ethexe_ethereum::Ethereum;
use ethexe_sdk::VaraEthApi;
use gprimitives::MessageId;
use parity_scale_codec::Encode;
use rand::{SeedableRng, rngs::SmallRng};
use std::str::FromStr;
use tracing::{debug, info, warn};

/// How much VARA (ERC-20 with 12 decimals) to give the mega contract.
const TOP_UP_AMOUNT: u128 = 500_000_000_000_000;

pub async fn run_fuzz(params: FuzzParams) -> Result<()> {
    let router_addr = Address::from_str(&params.router_address)?;

    let (signer, address) = if let Some(ref pk) = params.sender_private_key {
        crate::signer_from_private_key(pk)?
    } else {
        crate::signer_from_private_key(crate::DEPLOYER_ACCOUNT.private_key)?
    };

    info!("Fuzz deployer address: 0x{}", alloy::hex::encode(address.0));

    let api = Ethereum::new(&params.node, router_addr.into(), signer.clone(), address).await?;
    let vapi = VaraEthApi::new(&params.ethexe_node, api.clone()).await?;

    info!("Uploading mega syscall contract code...");
    let wasm = demo_syscalls_ethexe::WASM_BINARY;
    let (_, code_id) = vapi.router().request_code_validation(wasm).await?;
    vapi.router().wait_for_code_validation(code_id).await?;
    info!("Code validated: {code_id}");

    let salt_bytes = b"mega-fuzz-contract-v1";
    let salt_h256 = crate::batch::salt_to_h256(salt_bytes);
    let (_, program_id) = api
        .router()
        .create_program(code_id, salt_h256, None)
        .await?;
    info!("Program created: {program_id}");

    api.router()
        .wvara()
        .approve(program_id, TOP_UP_AMOUNT)
        .await?;
    let mirror = api.mirror(program_id);
    mirror.executable_balance_top_up(TOP_UP_AMOUNT).await?;
    info!("Program topped up");

    let init_config = InitConfig { echo_dest: None };
    let init_block = {
        use alloy::providers::Provider;
        api.provider().get_block_number().await?
    };
    let (_, init_mid) = mirror.send_message(&init_config.encode(), 0).await?;
    info!("Init message sent: {init_mid}");

    let init_outcome = wait_for_reply(&api, init_mid, init_block, 8).await?;
    match init_outcome {
        None => info!("Init processed successfully"),
        Some(err) => warn!("Init reply: {err}"),
    }

    // ── Step 4: fuzz loop ──
    let seed = params.seed.unwrap_or_else(gear_utils::now_millis);
    let mut rng = SmallRng::seed_from_u64(seed);
    info!("Fuzz seed: {seed}");

    let max_iter = if params.iterations == 0 {
        u64::MAX
    } else {
        params.iterations
    };

    let mut ok_count: u64 = 0;
    let mut err_count: u64 = 0;

    for i in 0..max_iter {
        let commands = cmd_gen::generate_fuzz_commands(&mut rng, params.max_commands, program_id);
        let cmd_count = commands.len();
        let payload = commands.encode();

        debug!(
            "Iteration {i}: sending {cmd_count} commands ({} bytes)",
            payload.len()
        );

        let start_block = {
            use alloy::providers::Provider;
            api.provider().get_block_number().await?
        };
        let (_, msg_id) = mirror.send_message(&payload, 0).await?;
        debug!("Message sent: {msg_id}");

        // Wait for processing
        let wait_blocks_count = 8;
        let outcome = wait_for_reply(&api, msg_id, start_block, wait_blocks_count).await;

        match outcome {
            Ok(None) => {
                ok_count += 1;
                debug!("Iteration {i}: SUCCESS");
            }
            Ok(Some(err_msg)) => {
                warn!("Iteration {i}: TRAP — {err_msg}");
                err_count += 1;
            }
            Err(e) => {
                warn!("Iteration {i}: ERROR waiting for reply — {e:?}");
                err_count += 1;
            }
        }

        info!(
            "Progress: {}/{max_iter} iterations, ok={ok_count}, err={err_count}",
            i + 1
        );
    }

    info!("Fuzz complete: {max_iter} iterations, ok={ok_count}, err={err_count}, seed={seed}");

    Ok(())
}

/// Wait for a reply to `msg_id` by polling blocks from `start_block` up to
/// `max_blocks` ahead. Sleeps up to 12 seconds between polls when waiting for
/// new blocks to appear.
/// Returns `Ok(None)` on success, `Ok(Some(err))` on error reply, `Err` on timeout.
async fn wait_for_reply(
    api: &Ethereum,
    msg_id: MessageId,
    start_block: u64,
    max_blocks: usize,
) -> Result<Option<String>> {
    use alloy::{providers::Provider, rpc::types::Filter};
    use ethexe_common::events::MirrorEvent;
    use ethexe_ethereum::mirror::events::try_extract_event;
    use gear_core::ids::prelude::MessageIdExt;

    let end_block = start_block + max_blocks as u64;
    let mut next_block = start_block;

    while next_block <= end_block {
        let latest = api.provider().get_block_number().await?;

        if next_block >= latest {
            tokio::time::sleep(std::time::Duration::from_secs(12)).await;
            continue;
        }

        let fetch_until = latest.min(end_block);
        let logs = api
            .provider()
            .get_logs(&Filter::new().from_block(next_block).to_block(fetch_until))
            .await?;

        for log in logs {
            if let Some(mirror_event) = try_extract_event(&log)? {
                match mirror_event {
                    MirrorEvent::Reply(reply) => {
                        if reply.reply_to == msg_id
                            || MessageId::generate_reply(reply.reply_to) == msg_id
                        {
                            if reply.reply_code.is_success() {
                                return Ok(None);
                            } else {
                                let err = String::from_utf8(reply.payload.clone())
                                    .unwrap_or_else(|_| "<non-utf8>".to_string());
                                return Ok(Some(err));
                            }
                        }
                    }
                    MirrorEvent::MessageCallFailed(call) if call.id == msg_id => {
                        return Ok(Some(format!(
                            "MessageCallFailed: dest={}, value={}",
                            call.destination, call.value
                        )));
                    }
                    MirrorEvent::ReplyCallFailed(call) if call.reply_to == msg_id => {
                        return Ok(Some(format!("ReplyCallFailed: code={}", call.reply_code)));
                    }
                    _ => {}
                }
            }
        }

        next_block = fetch_until + 1;
    }

    Ok(Some("TIMEOUT: no reply within block window".to_string()))
}
