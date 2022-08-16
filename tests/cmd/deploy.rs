//! Integration tests for command `deploy`
use crate::common::{self, logs, traits::Convert, Result};

#[tokio::test]
async fn test_command_deploy_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev(9002)?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = common::gear(&[
        "-e",
        "ws://127.0.0.1:9002",
        "deploy",
        "res/demo_meta.opt.wasm",
    ])?;

    assert!(output
        .stdout
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_PROGRAM));
    Ok(())
}
