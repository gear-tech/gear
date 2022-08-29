//! Integration tests for command `send`
use crate::common::{self, Result};
use gear_program::api::Api;
use parity_scale_codec::Encode;

#[tokio::test]
async fn test_command_reply_works() -> Result<()> {
    let node = common::create_messager().await?;

    // Get balance of the testing address
    let api = Api::new(Some(&node.ws()), None).await?;
    let mailbox = api.mailbox(common::alice_account_id(), 10).await?;
    assert_eq!(mailbox.len(), 1);
    let id = hex::encode(mailbox[0].0.id.0);

    // Send message to messager
    let _ = common::gear(&["-e", &node.ws(), "reply", &id, "0x", "20000000000"])?;
    let mailbox = api.mailbox(common::alice_account_id(), 10).await?;
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox[0].0.payload, messager::REPLY_REPLY.encode());

    Ok(())
}
