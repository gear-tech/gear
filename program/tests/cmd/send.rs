//! Integration tests for command `send`
use crate::common::{self, Result};
use gear_program::api::Api;
use parity_scale_codec::Encode;

#[tokio::test]
async fn test_command_send_works() -> Result<()> {
    let node = common::create_messager().await?;

    // Get balance of the testing address
    let api = Api::new(Some(&node.ws())).await?.try_signer(None)?;
    let mailbox = api.mailbox(common::alice_account_id(), 10).await?;
    assert_eq!(mailbox.len(), 1);
    let dest = hex::encode(mailbox[0].0.source.0);

    // Send message to messager
    let _ = common::gear(&["-e", &node.ws(), "send", &dest, "0x", "20000000000"])?;
    let mailbox = api.mailbox(common::alice_account_id(), 10).await?;
    assert_eq!(mailbox.len(), 2);
    assert!(mailbox
        .iter()
        .any(|mail| mail.0.payload == messager::SEND_REPLY.encode()));

    Ok(())
}
