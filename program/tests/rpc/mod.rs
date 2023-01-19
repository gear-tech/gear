use crate::common::{self, logs, Result};
use gear_program::api::Api;
use parity_scale_codec::Encode;

#[tokio::test]
async fn test_calculate_upload_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let api = Api::new(Some(&node.ws())).await?;
    let alice_account_id = common::alice_account_id();
    let alice: [u8; 32] = *alice_account_id.as_ref();

    api.calculate_upload_gas(
        alice.into(),
        messager::WASM_BINARY.to_vec(),
        vec![],
        0,
        true,
        None,
    )
    .await
    .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_create_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // 1. upload code.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer.upload_code(messager::WASM_BINARY.to_vec()).await?;

    // 2. calculate create gas and create program.
    let code_id = common::hash(messager::WASM_BINARY);
    let gas_info = signer
        .calculate_create_gas(None, code_id.into(), vec![], 0, true, None)
        .await?;

    signer
        .create_program(code_id.into(), vec![], vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_handle_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let salt = vec![];
    let pid = common::program_id(messager::WASM_BINARY, &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .upload_program(
            messager::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            10_000_000_000,
        )
        .await?;

    assert!(signer.gprog(pid.into()).await.is_ok());

    // 2. calculate handle gas and send message.
    let gas_info = signer
        .calculate_handle_gas(None, pid.into(), vec![], 0, true, None)
        .await?;

    signer
        .send_message(pid.into(), vec![], gas_info.min_limit, 0)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let alice_account_id = common::alice_account_id();
    let alice: [u8; 32] = *alice_account_id.as_ref();
    let salt = vec![];
    let pid = common::program_id(demo_waiter::WASM_BINARY, &salt);
    let payload = demo_waiter::Command::SendUpTo(alice.into(), 10);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer
        .upload_program(
            demo_waiter::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(signer.gprog(pid.into()).await.is_ok());

    // 2. send wait message.
    signer
        .send_message(pid.into(), payload.encode(), 100_000_000_000, 0)
        .await?;

    let mailbox = signer.mailbox(alice_account_id, 10).await?;
    assert_eq!(mailbox.len(), 1);
    let message_id = mailbox[0].0.id.clone().into();

    // 3. calculate reply gas and send reply.
    let gas_info = signer
        .calculate_reply_gas(None, message_id, 1, vec![], 0, true, None)
        .await?;

    signer
        .send_reply(message_id, vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}
