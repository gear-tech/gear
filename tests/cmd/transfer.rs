//! Integration tests for command `deploy`
use crate::common::{self, logs, Result};
use gear_program::api::Api;

// Testing account
//
// Secret phrase:     tumble tenant update heavy sad draw present tray atom chunk animal exhaust
// Network ID:        substrate
// Secret seed:       0xd13d64420f7e304a1bfd4a17a5cda3f14b4e98034abe2cbd4fc05214c6ba2488
// Public key (hex):  0x62bd03f963e636deea9139b00e33e6800f3c1afebb5f69b47ed07c07be549e78
// Account ID:        0x62bd03f963e636deea9139b00e33e6800f3c1afebb5f69b47ed07c07be549e78
// Public key (SS58): 5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs
// SS58 Address:      5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs
const SURI: &str = "tumble tenant update heavy sad draw present tray atom chunk animal exhaust";
const ADDRESS: &str = "5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs";

#[tokio::test]
async fn test_command_transfer_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // Get balance of the testing address
    let api = Api::new(Some(&node.ws())).await?.signer(SURI, None)?;
    let before = api.get_balance(ADDRESS).await?;

    // Run command transfer
    let value = 1000000000_u128;
    let _ = common::gear(&["-e", &node.ws(), "transfer", ADDRESS, &value.to_string()])?;
    let after = api.get_balance(ADDRESS).await?;

    assert_eq!(after.saturating_sub(before), value);
    Ok(())
}
