//! Integration tests for command `deploy`
use crate::common::{self, logs, traits::Convert, Result};

#[tokio::test]
async fn test_command_create_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = common::gear(&["-e", &node.ws(), "create", "res/demo_meta.opt.wasm"])?;
    assert!(output
        .stderr
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_PROGRAM));

    Ok(())
}
