use demo_backend_error::WASM_BINARY;
use gclient::{EventProcessor, GearApi, Node};

const GEAR_PATH: &str = "../target/release/gear";

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn two_nodes_run_independently() {
    let node_1 = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate dev node 1.");
    let node_2 = Node::try_from_path(GEAR_PATH).expect("Unable to instantiate dev node 2.");
    let salt = gclient::now_in_micros().to_le_bytes();

    // The assumption is that it is not allowed to load the same code with the same
    // salt to the same node twice
    upload_program_to_node(&node_1, WASM_BINARY, &salt).await;
    upload_program_to_node(&node_2, WASM_BINARY, &salt).await;
}

async fn upload_program_to_node(node: &Node, code: &[u8], salt: &[u8]) {
    let api = GearApi::node(node)
        .await
        .expect("Unable to instantiate api.");

    let gas_limit = api
        .block_gas_limit()
        .expect("Unable to obtain block gas limit.");

    let mut listener = api
        .subscribe()
        .await
        .expect("Unable to subscribe to node events.");

    let (mid, _pid, _) = api
        .upload_program_bytes(code, salt, [], gas_limit, 0)
        .await
        .expect("Unable to load a program.");

    // Asserting successful initialization.
    assert!(listener
        .message_processed(mid)
        .await
        .expect("Unable to obtain confirmation on processed message.")
        .succeed());
}
