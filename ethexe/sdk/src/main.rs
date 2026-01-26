use anyhow::{Context, Result};
use ethexe_common::{
    Address,
    events::{
        router::BatchCommittedEvent,
        wvara::{ApprovalEvent, TransferEvent},
    },
};
use ethexe_ethereum::{Ethereum, router::CodeValidationResult};
use ethexe_runtime_common::state::ProgramState;
use ethexe_sdk::VaraEthApi;
use ethexe_signer::Signer;
use futures::StreamExt;
use gear_core::message::{ReplyCode, SuccessReplyReason};
use gprimitives::{H256, U256};
use std::{env, fs};

#[tokio::main]
async fn main() -> Result<()> {
    let ethereum_rpc_url = "ws://localhost:8545";
    let router_address: Address = "0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9".parse()?;
    let home_dir = env::home_dir().context("failed to get home directory")?;
    let signer = Signer::fs(home_dir.join(".local/share/ethexe/keys"));
    let sender_address: Address = "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC".parse()?;
    let ethereum_client = Ethereum::new(
        ethereum_rpc_url,
        router_address,
        signer.clone(),
        sender_address,
    )
    .await?;

    let vara_eth_rpc_url = "ws://localhost:9944";
    let vara_eth_api = VaraEthApi::new(vara_eth_rpc_url, ethereum_client).await?;

    let sender_address2: Address = "0x90F79bf6EB2c4f870365E785982E1f101E93b906".parse()?;
    let ethereum_client2 =
        Ethereum::new(ethereum_rpc_url, router_address, signer, sender_address2).await?;
    let vara_eth_api2 = VaraEthApi::new(vara_eth_rpc_url, ethereum_client2).await?;

    // WrappedVara.sol tests

    const WVARA_UNIT: u128 = 1_000_000_000_000;

    assert_eq!(vara_eth_api.wrapped_vara().name().await?, "Wrapped Vara");
    assert_eq!(vara_eth_api.wrapped_vara().symbol().await?, "WVARA");
    assert_eq!(vara_eth_api.wrapped_vara().decimals().await?, 12);

    assert_eq!(
        vara_eth_api.wrapped_vara().total_supply().await?,
        6_500_000 * WVARA_UNIT
    );

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address)
            .await?,
        500_000 * WVARA_UNIT
    );

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address2)
            .await?,
        500_000 * WVARA_UNIT
    );

    let mut stream = vara_eth_api
        .wrapped_vara()
        .events()
        .transfer()
        .from(sender_address.into())
        .to(sender_address2.into())
        .subscribe()
        .await?
        .take(1);

    vara_eth_api
        .wrapped_vara()
        .transfer(sender_address2, 100_000 * WVARA_UNIT)
        .await?;

    while let Some(result) = stream.next().await {
        if let Ok((TransferEvent { from, to, value }, _)) = result {
            assert_eq!(from, sender_address.into());
            assert_eq!(to, sender_address2.into());
            assert_eq!(value, 100_000 * WVARA_UNIT);
            break;
        }
    }

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address)
            .await?,
        400_000 * WVARA_UNIT
    );

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address2)
            .await?,
        600_000 * WVARA_UNIT
    );

    let mut stream = vara_eth_api
        .wrapped_vara()
        .events()
        .approval()
        .owner(sender_address.into())
        .spender(sender_address2.into())
        .subscribe()
        .await?
        .take(1);

    vara_eth_api
        .wrapped_vara()
        .approve(sender_address2, 100_000 * WVARA_UNIT)
        .await?;

    while let Some(result) = stream.next().await {
        if let Ok((
            ApprovalEvent {
                owner,
                spender,
                value,
            },
            _,
        )) = result
        {
            assert_eq!(owner, sender_address.into());
            assert_eq!(spender, sender_address2.into());
            assert_eq!(value, U256::from(100_000 * WVARA_UNIT));
            break;
        }
    }

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .allowance(sender_address, sender_address2)
            .await?,
        U256::from(100_000 * WVARA_UNIT)
    );

    vara_eth_api2
        .wrapped_vara()
        .transfer_from(sender_address, sender_address2, 50_000 * WVARA_UNIT)
        .await?;

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address)
            .await?,
        350_000 * WVARA_UNIT
    );

    assert_eq!(
        vara_eth_api
            .wrapped_vara()
            .balance_of(sender_address2)
            .await?,
        650_000 * WVARA_UNIT
    );

    // Router.sol tests

    let code = fs::read("./target/wasm32-gear/debug/demo_ping.opt.wasm")?;
    let (_, code_id) = vara_eth_api.router().request_code_validation(&code).await?;

    let CodeValidationResult {
        valid,
        tx_hash,
        block_hash,
        block_number,
    } = vara_eth_api
        .router()
        .wait_for_code_validation(code_id)
        .await?;

    assert!(valid);
    assert!(tx_hash.is_some());
    assert!(block_hash.is_some());
    assert!(block_number.is_some());

    let salt = H256::zero();
    let (_, actor_id) = vara_eth_api
        .router()
        .create_program(code_id, salt, None)
        .await?;

    let mut stream = vara_eth_api
        .router()
        .events()
        .batch_committed()
        .subscribe()
        .await?;

    while let Some(result) = stream.next().await {
        if let Ok((BatchCommittedEvent { .. }, _)) = result {
            break;
        }
    }

    let program_ids = vara_eth_api.router().program_ids().await?;
    assert!(program_ids.contains(&actor_id));

    let programs_count = vara_eth_api.router().programs_count().await?;
    assert_eq!(programs_count, 1);

    // Mirror.sol tests

    let mirror = vara_eth_api.mirror(actor_id);

    assert_eq!(mirror.code_id().await?, code_id);

    const ETHER: u128 = 1_000_000_000_000_000_000;

    let new_balance = 5 * ETHER;
    mirror.owned_balance_top_up(new_balance).await?;

    mirror.wait_for_state_change().await?;

    let state = mirror.state().await?;
    let ProgramState {
        balance,
        executable_balance,
        ..
    } = state;

    assert_eq!(balance, new_balance);
    assert_eq!(executable_balance, 0);

    let new_executable_balance = 100 * WVARA_UNIT;

    vara_eth_api
        .wrapped_vara()
        .approve(mirror.address(), new_executable_balance)
        .await?;

    mirror
        .executable_balance_top_up(new_executable_balance)
        .await?;

    mirror.wait_for_state_change().await?;

    let state = mirror.state().await?;
    let ProgramState {
        balance,
        executable_balance,
        ..
    } = state;

    assert_eq!(balance, new_balance);
    assert_eq!(executable_balance, new_executable_balance);

    let (_, message_id) = mirror.send_message(b"PING", 0).await?;

    let reply_info = mirror.wait_for_reply(message_id).await?;
    assert_eq!(reply_info.message_id, message_id);
    assert_eq!(reply_info.actor_id, mirror.address().into());
    assert_eq!(reply_info.payload, b"PONG".to_vec());
    assert_eq!(
        reply_info.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(reply_info.value, 0);

    let message_id = mirror.send_message_injected(b"PING", 0).await?;

    let reply_info = mirror.wait_for_reply(message_id).await?;
    assert_eq!(reply_info.message_id, message_id);
    assert_eq!(reply_info.actor_id, mirror.address().into());
    assert_eq!(reply_info.payload, b"PONG".to_vec());
    assert_eq!(
        reply_info.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    let (message_id, promise) = mirror.send_message_injected_and_watch(b"PING", 0).await?;
    assert_eq!(promise.tx_hash.inner(), H256::from(message_id));
    assert_eq!(promise.reply.payload, b"PONG".to_vec());
    assert_eq!(promise.reply.value, 0);
    assert_eq!(
        promise.reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    Ok(())
}
