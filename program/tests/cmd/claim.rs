//! Integration tests for command `send`
use crate::common::{self, Result, ALICE_SS58_ADDRESS as ADDRESS};
use gear_program::api::Api;

const REWARD_PER_BLOCK: u128 = 3_000_000; // 3_000 gas * 1_000 value per gas

#[tokio::test]
async fn test_command_claim_works() -> Result<()> {
    let node = common::create_messager().await?;

    // Check the mailbox of the testing account
    let api = Api::new(Some(&node.ws())).await?.try_signer(None)?;
    let mailbox = api.mailbox(common::alice_account_id(), 10).await?;

    assert_eq!(mailbox.len(), 1);
    let id = hex::encode(mailbox[0].0.id.0);

    // Claim value from message id
    let before = api.get_balance(ADDRESS).await?;
    let _ = common::gear(&["-e", &node.ws(), "claim", &id])?;
    let after = api.get_balance(ADDRESS).await?;

    // # TODO
    //
    // not using `//Alice` or estimating the reward
    // before this checking.
    assert_eq!(
        after.saturating_sub(before),
        messager::SENT_VALUE + REWARD_PER_BLOCK
    );

    Ok(())
}
