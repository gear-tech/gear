use futures::prelude::*;

use std::pin::pin;

use gear_core::ids::{ActorId, MessageId};
use gear_node_wrapper::{Node, NodeInstance};
use gsdk::{
    Api, Error, Event, Result, SignedApi,
    gear::{constants, gear, runtime_types::gear_common::event::DispatchStatus},
};
use parity_scale_codec::{Decode, Encode};

const GEAR_PATH: &str = "../target/release/gear";

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn two_nodes_run_independently() -> gsdk::Result<()> {
    let salt = gear_utils::now_micros().to_le_bytes();

    let (_node_a, api_a) = run_node().await;
    let (_node_b, api_b) = run_node().await;

    upload_program_to_node(&api_a, demo_mul_by_const::WASM_BINARY, &salt, 42u64).await;
    upload_program_to_node(&api_b, demo_mul_by_const::WASM_BINARY, &salt, 43u64).await;

    Ok(())
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_migrated_to_another_node() {
    const INIT_VALUE_PAYLOAD: u64 = 42;
    const MULTIPLICATOR_VALUE_PAYLOAD: u64 = 4;
    const PROGRAM_FUNDS: u128 = 25_000_000_000_000;
    const ED: u128 = 1_000_000_000_000;

    // Arrange

    // Upload source program to the source node
    let (_src_node, src_node_api) = run_node().await;

    let src_program_id = upload_program_to_node(
        &src_node_api,
        demo_mul_by_const::WASM_BINARY,
        &gear_utils::now_micros().to_le_bytes(),
        INIT_VALUE_PAYLOAD,
    )
    .await;

    // Transfer some funds to the source program
    src_node_api
        .transfer_keep_alive(src_program_id, PROGRAM_FUNDS)
        .await
        .expect("Unable to transfer funds to source program");

    // Initialize destination node
    let (_dest_node, dest_node_api) = run_node().await;

    let dest_node_gas_limit = dest_node_api
        .constants()
        .at(&constants().gear_gas().block_gas_limit())
        .expect("Unable to get destination node gas limit");

    let dest_node_events = dest_node_api
        .subscribe_all_events()
        .await
        .expect("Unable to subscribe to destination node events");

    // Act

    // Migrate the source program onto the destination node
    let dest_program_id = src_node_api
        .migrate_program(src_program_id, None, &dest_node_api)
        .await
        .expect("Unable to migrate source program");

    // Send some message to the destination program for checking that it
    // functions properly
    let message_id = dest_node_api
        .send_message(
            dest_program_id,
            MULTIPLICATOR_VALUE_PAYLOAD,
            dest_node_gas_limit,
            0,
        )
        .await
        .expect("Unable to send message to destination program")
        .value;

    // Assert
    assert_eq!(src_program_id, dest_program_id);

    let dest_program_funds = dest_node_api
        .unsigned()
        .free_balance(dest_program_id)
        .await
        .expect("Unable to get destination program funds");
    assert_eq!(dest_program_funds, PROGRAM_FUNDS + ED);

    let dest_program_reply = u64::decode(
        &mut reply_bytes_on(message_id, dest_node_events)
            .await
            .as_slice(),
    )
    .unwrap();
    assert_eq!(
        INIT_VALUE_PAYLOAD * MULTIPLICATOR_VALUE_PAYLOAD,
        dest_program_reply
    );
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_migration_fails_if_program_exists() {
    const INIT_VALUE_PAYLOAD: u64 = 42;

    // Arrange

    let (_src_node, src_node_api) = run_node().await;

    // Upload source program to the source node
    let src_program_id = upload_program_to_node(
        &src_node_api,
        demo_mul_by_const::WASM_BINARY,
        &gear_utils::now_micros().to_le_bytes(),
        INIT_VALUE_PAYLOAD,
    )
    .await;

    // Initialize destination node
    let (_dest_node, dest_node_api) = run_node().await;

    // Migrate the source program onto the destination node
    src_node_api
        .migrate_program(src_program_id, None, &dest_node_api)
        .await
        .expect("Unable to migrate source program");

    // Act: migrate the source program onto the source node

    let migration_error = src_node_api
        .migrate_program(src_program_id, None, &src_node_api)
        .await
        .expect_err("Unexpected migration result");

    // Assert

    if let Error::ProgramAlreadyExists(existing_program_id) = migration_error {
        assert_eq!(src_program_id, existing_program_id);
    } else {
        unreachable!("Unexpected migration error: {:?}", migration_error)
    }

    // Act: migrate the source program onto the destination node second time

    let migration_error = src_node_api
        .migrate_program(src_program_id, None, &dest_node_api)
        .await
        .expect_err("Unexpected migration result");

    // Assert

    if let Error::ProgramAlreadyExists(existing_program_id) = migration_error {
        assert_eq!(src_program_id, existing_program_id);
    } else {
        unreachable!("Unexpected migration error: {:?}", migration_error)
    }
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_with_gas_reservation_migrated_to_another_node() {
    // Arrange

    let (_src_node, src_node_api) = run_node().await;

    // Upload source program to the source node
    let src_program_id = upload_program_to_node(
        &src_node_api,
        demo_reserve_gas::WASM_BINARY,
        &gear_utils::now_micros().to_le_bytes(),
        demo_reserve_gas::InitAction::Normal(vec![
            // orphan reservation; will be removed automatically
            (50_000, 3),
            // must be cleared during `gr_exit`
            (25_000, 5),
        ]),
    )
    .await;

    let src_node_block_hash = src_node_api.blocks().at_latest().await.unwrap().hash();

    let (_dest_node, dest_node_api) = run_node().await;

    let dest_node_gas_limit = dest_node_api
        .constants()
        .at(&constants().gear_gas().block_gas_limit())
        .expect("Unable to get destination node gas limit");

    let dest_node_events = dest_node_api
        .subscribe_all_events()
        .await
        .expect("Unable to subscribe to destination node events");

    // Act

    // Migrate the source program onto the destination node
    let dest_program_id = src_node_api
        .migrate_program(src_program_id, Some(src_node_block_hash), &dest_node_api)
        .await
        .expect("Unable to migrate source program");

    let message_id = dest_node_api
        .send_message(
            dest_program_id,
            demo_reserve_gas::HandleAction::ReplyFromReservation,
            dest_node_gas_limit,
            0,
        )
        .await
        .expect("Unable to send message to destination program")
        .value;

    // Assert

    let dest_program_reply = reply_bytes_on(message_id, dest_node_events).await;
    assert_eq!(
        demo_reserve_gas::REPLY_FROM_RESERVATION_PAYLOAD.as_ref(),
        &dest_program_reply
    );
}

async fn run_node() -> (NodeInstance, SignedApi) {
    let node = Node::from_path(GEAR_PATH).unwrap().spawn().unwrap();

    let api = Api::new(&node.ws()).await.unwrap().signed_as_alice();

    (node, api)
}

async fn reply_bytes_on(
    message_id: MessageId,
    events: impl Stream<Item = Result<Event>>,
) -> Vec<u8> {
    let payloads = events
        .map(|event| {
            Ok::<_, gsdk::Error>(
                if let Event::Gear(gear::Event::UserMessageSent { message, .. }) = event?
                    && let Some(details) = message.details()
                    && details.to_message_id() == message_id
                {
                    let payload = message.payload_bytes();

                    if details.to_reply_code().is_success() {
                        Some(payload.to_vec())
                    } else {
                        panic!(
                            "reply contains an error: {:?}",
                            std::str::from_utf8(payload).unwrap()
                        )
                    }
                } else {
                    None
                },
            )
        })
        .filter_map(|res| future::ready(res.transpose()));

    pin!(payloads).next().await.unwrap().unwrap()
}

async fn upload_program_to_node<E>(api: &SignedApi, code: &[u8], salt: &[u8], payload: E) -> ActorId
where
    E: Encode,
{
    let gas_limit = api
        .constants()
        .at(&constants().gear_gas().block_gas_limit())
        .expect("Unable to obtain block gas limit");

    let events = api.subscribe_all_events().await.unwrap();

    let (mid, pid) = api
        .upload_program(code, salt, payload, gas_limit, 0)
        .await
        .expect("Unable to load a program")
        .value;

    let dispatch_statuses = events
        .map(|event| {
            Ok::<_, gsdk::Error>(match event? {
                Event::Gear(gear::Event::MessagesDispatched { statuses, .. }) => statuses
                    .into_iter()
                    .find_map(|(message_id, status)| (message_id == mid).then_some(status)),
                _ => None,
            })
        })
        .filter_map(|res| async move { res.transpose() });
    let dispatch_status = pin!(dispatch_statuses).next().await.unwrap().unwrap();

    // Asserting successful initialization.
    assert_eq!(dispatch_status, DispatchStatus::Success);

    pid
}
