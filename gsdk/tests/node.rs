use gear_core::ids::ActorId;
use gsdk::{Error, SignedApi, events, gear::constants};
use parity_scale_codec::Encode;
use utils::dev_node;

mod utils;

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn two_nodes_run_independently() -> gsdk::Result<()> {
    let salt = gear_utils::now_micros().to_le_bytes();

    let (_node_a, api_a) = dev_node().await;
    let (_node_b, api_b) = dev_node().await;

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
    let (_src_node, src_node_api) = dev_node().await;

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
    let (_dest_node, dest_node_api) = dev_node().await;

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

    let dest_program_reply: u64 = events::reply_on(message_id, dest_node_events)
        .await
        .unwrap()
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

    let (_src_node, src_node_api) = dev_node().await;

    // Upload source program to the source node
    let src_program_id = upload_program_to_node(
        &src_node_api,
        demo_mul_by_const::WASM_BINARY,
        &gear_utils::now_micros().to_le_bytes(),
        INIT_VALUE_PAYLOAD,
    )
    .await;

    // Initialize destination node
    let (_dest_node, dest_node_api) = dev_node().await;

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

    let (_src_node, src_node_api) = dev_node().await;

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

    let (_dest_node, dest_node_api) = dev_node().await;

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

    let dest_program_reply = events::reply_bytes_on(message_id, dest_node_events)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        demo_reserve_gas::REPLY_FROM_RESERVATION_PAYLOAD.as_ref(),
        &dest_program_reply
    );
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

    let (message_id, pid) = api
        .upload_program(code, salt, payload, gas_limit, 0)
        .await
        .expect("Unable to load a program")
        .value;

    // Asserting successful initialization.
    assert!(
        events::message_dispatch_status(message_id, events)
            .await
            .unwrap()
            .is_success(),
    );

    pid
}
