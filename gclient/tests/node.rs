use gclient::{Error, EventProcessor, GearApi};
use gear_core::ids::ProgramId;
use hex::ToHex;
use parity_scale_codec::{Decode, Encode};

const GEAR_PATH: &str = "../target/release/gear";

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn two_nodes_run_independently() {
    let salt = gclient::now_micros().to_le_bytes();

    // The assumption is that it is not allowed to load the same code with the same
    // salt to the same node twice
    upload_program_to_node(demo_mul_by_const::WASM_BINARY, &salt, Some(42u64)).await;
    upload_program_to_node(demo_mul_by_const::WASM_BINARY, &salt, Some(43u64)).await;
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_migrated_to_another_node() {
    const INIT_VALUE_PAYLOAD: u64 = 42;
    const MULTIPLICATOR_VALUE_PAYLOAD: u64 = 4;
    const PROGRAM_FUNDS: u128 = 25_000;

    // Arrange

    // Upload source program to the source node
    let (src_node_api, src_program_id) = upload_program_to_node(
        demo_mul_by_const::WASM_BINARY,
        &gclient::now_micros().to_le_bytes(),
        Some(INIT_VALUE_PAYLOAD),
    )
    .await;

    // Transfer some funds to the source program
    src_node_api
        .transfer(src_program_id, PROGRAM_FUNDS)
        .await
        .expect("Unable to transfer funds to source program");

    // Initialize destination node
    let dest_node_api = GearApi::dev_from_path(GEAR_PATH)
        .await
        .expect("Unable to connect to destination node api");

    let dest_node_gas_limit = dest_node_api
        .block_gas_limit()
        .expect("Unable to get destination node gas limit");

    let mut dest_node_listener = dest_node_api
        .subscribe()
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
    let (message_id, _) = dest_node_api
        .send_message(
            dest_program_id,
            MULTIPLICATOR_VALUE_PAYLOAD,
            dest_node_gas_limit,
            0,
        )
        .await
        .expect("Unable to send message to destination program");

    // Assert
    assert_eq!(src_program_id, dest_program_id);

    let dest_program_funds = dest_node_api
        .free_balance(dest_program_id)
        .await
        .expect("Unable to get destination program funds");
    assert_eq!(dest_program_funds, PROGRAM_FUNDS);

    let dest_program_reply = dest_node_listener
        .reply_bytes_on(message_id)
        .await
        .expect("Unable to get reply from destination program")
        .1
        .expect("Unable to read reply payload");
    assert_eq!(
        INIT_VALUE_PAYLOAD * MULTIPLICATOR_VALUE_PAYLOAD,
        u64::decode(&mut dest_program_reply.as_ref()).expect("Unable to decode reply payload")
    );
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_migration_fails_if_program_exists() {
    const INIT_VALUE_PAYLOAD: u64 = 42;

    // Arrange

    // Upload source program to the source node
    let (src_node_api, src_program_id) = upload_program_to_node(
        demo_mul_by_const::WASM_BINARY,
        &gclient::now_micros().to_le_bytes(),
        Some(INIT_VALUE_PAYLOAD),
    )
    .await;

    // Initialize destination node
    let dest_node_api = GearApi::dev_from_path(GEAR_PATH)
        .await
        .expect("Unable to connect to destination node api");

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
        assert_eq!(
            src_program_id.as_ref().encode_hex::<String>(),
            existing_program_id
        );
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
        assert_eq!(
            src_program_id.as_ref().encode_hex::<String>(),
            existing_program_id
        );
    } else {
        unreachable!("Unexpected migration error: {:?}", migration_error)
    }
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_with_gas_reservation_migrated_to_another_node() {
    // Arrange

    // Upload source program to the source node
    let (src_node_api, src_program_id) = upload_program_to_node(
        demo_reserve_gas::WASM_BINARY,
        &gclient::now_micros().to_le_bytes(),
        Some(demo_reserve_gas::InitAction::Normal),
    )
    .await;

    let src_node_block_hash = src_node_api
        .last_block_hash()
        .await
        .expect("Unable to get source node last block hash");

    // Initialize the destination node api
    let dest_node_api = GearApi::dev_from_path(GEAR_PATH)
        .await
        .expect("Unable to connect to destination node api");

    let dest_node_gas_limit = dest_node_api
        .block_gas_limit()
        .expect("Unable to get destination node gas limit");

    let mut dest_node_listener = dest_node_api
        .subscribe()
        .await
        .expect("Unable to subscribe to destination node events");

    // Act

    // Migrate the source program onto the destination node
    let dest_program_id = src_node_api
        .migrate_program(src_program_id, Some(src_node_block_hash), &dest_node_api)
        .await
        .expect("Unable to migrate source program");

    let (message_id, _) = dest_node_api
        .send_message(
            dest_program_id,
            demo_reserve_gas::HandleAction::ReplyFromReservation,
            dest_node_gas_limit,
            0,
        )
        .await
        .expect("Unable to send message to destination program");

    // Assert

    let dest_program_reply = dest_node_listener
        .reply_bytes_on(message_id)
        .await
        .expect("Unable to get reply from destination program")
        .1
        .expect("Unable to read reply payload");
    assert_eq!(
        demo_reserve_gas::REPLY_FROM_RESERVATION_PAYLOAD.as_ref(),
        dest_program_reply
    );
}

async fn upload_program_to_node<E>(
    code: &[u8],
    salt: &[u8],
    init_payload: Option<E>,
) -> (GearApi, ProgramId)
where
    E: Encode,
{
    let api = GearApi::dev_from_path(GEAR_PATH)
        .await
        .expect("Unable to connect to node api");

    let gas_limit = api
        .block_gas_limit()
        .expect("Unable to obtain block gas limit");

    let mut listener = api
        .subscribe()
        .await
        .expect("Unable to subscribe to node events");

    let (mid, pid, _) = api
        .upload_program_bytes(
            code,
            salt,
            init_payload.map_or(vec![], |p| p.encode()),
            gas_limit,
            0,
        )
        .await
        .expect("Unable to load a program");

    // Asserting successful initialization.
    assert!(listener
        .message_processed(mid)
        .await
        .expect("Unable to obtain confirmation on processed message")
        .succeed());

    (api, pid)
}
