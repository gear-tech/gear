//! Integration tests for command `deploy`
use crate::common::{self, logs, traits::Convert, Result};

const EXPECTED: &str = r#"
AccountInfo {
    nonce: 0,
    consumers: 0,
    providers: 1,
    sufficients: 0,
    data: AccountData {
        free: 1152921504606846976,
        reserved: 0,
        misc_frozen: 0,
        fee_frozen: 0,
    },
}
"#;

#[tokio::test]
async fn test_command_info_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev(9001)?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = common::gear(&["-e", "ws://127.0.0.1:9001", "info", "//Alice"])?;
    assert_eq!(EXPECTED.trim(), output.stdout.convert().trim());
    Ok(())
}
