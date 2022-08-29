//! Integration tests for command `upload`

use crate::common::{self, logs, Result};
use gear_program::api::Api;

#[tokio::test]
async fn test_command_upload_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let api = Api::new(Some(&node.ws()), None).await?;
    let code_hash = common::hash(include_bytes!("../../res/demo_meta.opt.wasm"));
    assert!(api.code_storage(code_hash).await?.is_none());

    let _ = common::gear(&["-e", &node.ws(), "upload", "res/demo_meta.opt.wasm"])?;
    assert!(api.code_storage(code_hash).await?.is_some());

    Ok(())
}
