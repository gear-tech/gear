//! Integration tests for command `deploy`
use crate::common::{self, logs, traits::Convert, Result, ALICE_SS58_ADDRESS};

const EXPECTED_BALANCE: &str = r#"
AccountInfo {
    nonce: 0,
    consumers: 1,
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

const EXPECTED_MAILBOX: &str = r#"
    destination: "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
    payload: "0x",
    value: 1000000,
    details: None,
    interval: Interval {
        start: 2,
        finish: 31,
    },
}
"#;

#[tokio::test]
async fn test_action_balance_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = common::gear(&["-e", &node.ws(), "info", "//Alice", "balance"])?;
    assert_eq!(EXPECTED_BALANCE.trim(), output.stdout.convert().trim());
    Ok(())
}

#[tokio::test]
async fn test_action_mailbox_works() -> Result<()> {
    let node = common::create_messager().await?;
    let output = common::gear(&["-e", &node.ws(), "info", ALICE_SS58_ADDRESS, "mailbox"])?;

    assert!(output.stdout.convert().contains(EXPECTED_MAILBOX.trim()));
    Ok(())
}
