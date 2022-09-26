//! Integration tests for command `send`
use crate::common::{self, ALICE_SS58_ADDRESS as ADDRESS};
use gear_program::api::Api;

const REWARD_PER_BLOCK: u128 = 3000;

#[tokio::test]
async fn test_command_claim_works() {
    let node = common::create_messager()
        .await
        .expect("create messger failed");

    // Check the mailbox of the testing account
    let api = Api::new(Some(&node.ws()))
        .await
        .expect("New api failed")
        .try_signer(None)
        .expect("get signer failed");
    let mailbox = api
        .mailbox(common::alice_account_id(), 10)
        .await
        .expect("fetch mailbox failed");
    assert_eq!(mailbox.len(), 1);
    let id = hex::encode(mailbox[0].0.id.0);

    // Claim value from message id
    let before = api.get_balance(ADDRESS).await.expect("get balance failed");
    let _ = common::gear(&["-e", &node.ws(), "claim", &id]).expect("claim failed");
    let after = api.get_balance(ADDRESS).await.expect("get balance failed");

    // # TODO
    //
    // not using `//Alice` or estimating the reward
    // before this checking.
    assert_eq!(
        after.saturating_sub(before),
        messager::SENT_VALUE + REWARD_PER_BLOCK
    );
}
