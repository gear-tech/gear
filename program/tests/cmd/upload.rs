//! Integration tests for command `upload`

use crate::common::{self, env, logs};
use gear_program::api::Api;

#[tokio::test]
async fn test_command_upload_works() {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev().expect("failed to start node");
    node.wait(logs::gear_node::IMPORTING_BLOCKS)
        .expect("node timeout");

    let api = Api::new(Some(&node.ws()))
        .await
        .expect("build api failed")
        .try_signer(None)
        .expect("get signer failed");

    let code_hash = common::hash(demo_meta::WASM_BINARY);
    assert!(api
        .code_storage(code_hash)
        .await
        .expect("get code failed")
        .is_none());

    let _ = common::gear(&[
        "-e",
        &node.ws(),
        "upload",
        &env::wasm_bin("demo_meta.opt.wasm"),
    ])
    .expect("run command upload failed");

    assert!(api
        .code_storage(code_hash)
        .await
        .expect("get code failed")
        .is_some());
}
