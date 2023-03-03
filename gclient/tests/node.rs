use gclient::{EventProcessor, GearApi, GearApiWithNode, Node};
use gear_core::ids::ProgramId;
use parity_scale_codec::{Decode, Encode};

const GEAR_PATH: &str = "../target/release/gear";

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn two_nodes_run_independently() {
    let node_1 = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate dev node 1");
    let node_2 = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate dev node 2");
    let salt = gclient::now_in_micros().to_le_bytes();

    // The assumption is that it is not allowed to load the same code with the same
    // salt to the same node twice
    upload_program_to_node(&node_1, demo_mul_by_const::WASM_BINARY, &salt, Some(42u64)).await;
    upload_program_to_node(&node_2, demo_mul_by_const::WASM_BINARY, &salt, Some(43u64)).await;
}

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn program_migrated_to_another_node() {
    const INIT_VALUE: u64 = 42;
    const MULTIPLICATOR_VALUE: u64 = 4;
    const PROGRAM_FUNDS: u128 = 25_000;

    let src_node = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate source node");
    let tgt_node = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate target node");

    // Arrange
    let (src_node_api, src_program_id) = upload_program_to_node(
        &src_node,
        demo_mul_by_const::WASM_BINARY,
        &gclient::now_in_micros().to_le_bytes(),
        Some(INIT_VALUE),
    )
    .await;

    src_node_api
        .transfer(src_program_id, PROGRAM_FUNDS)
        .await
        .expect("Unable to transfer funds to source program");

    let tgt_node_api = GearApi::node(&tgt_node)
        .await
        .expect("Unable to connect to target node api");

    let tgt_node_gas_limit = tgt_node_api
        .block_gas_limit()
        .expect("Unable to get target node gas limit");

    let mut tgt_node_listener = tgt_node_api
        .subscribe()
        .await
        .expect("Unable to subscribe to target node events");

    // Act
    let tgt_program_id = src_node_api
        .migrate_program(src_program_id, &tgt_node_api)
        .await
        .expect("Unable to migrate source program");

    let (message_id, _) = tgt_node_api
        .send_message(tgt_program_id, MULTIPLICATOR_VALUE, tgt_node_gas_limit, 0)
        .await
        .expect("Unable to send message to target program");

    // Assert
    let tgt_program_funds = tgt_node_api
        .free_balance(tgt_program_id)
        .await
        .expect("Unable to get target program funds");
    assert_eq!(tgt_program_funds, PROGRAM_FUNDS);

    let tgt_program_reply = tgt_node_listener
        .reply_bytes_on(message_id)
        .await
        .expect("Unable to get reply from target program")
        .1
        .expect("Unable to read reply payload");
    assert_eq!(
        INIT_VALUE * MULTIPLICATOR_VALUE,
        u64::decode(&mut tgt_program_reply.as_ref()).expect("Unable to decode reply payload")
    );
}

async fn upload_program_to_node<'a, E>(
    node: &'a Node,
    code: &[u8],
    salt: &[u8],
    init_payload: Option<E>,
) -> (GearApiWithNode<'a>, ProgramId)
where
    E: Encode,
{
    let api = GearApi::node(node)
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
