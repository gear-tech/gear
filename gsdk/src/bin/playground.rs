use std::fs;

use gsdk::AccountKeyring;

#[tokio::main(flavor = "current_thread")]
async fn main() -> gsdk::Result<()> {
    let api = gsdk::Api::builder()
        .dev()
        .build()
        .await?
        .signed_dev(AccountKeyring::Alice);

    api.upload_program_bytes(
        fs::read("target/wasm32-gear/debug/demo_messenger.opt.wasm").unwrap(),
        gear_utils::now_micros().to_le_bytes(),
        vec![],
        api.block_gas_limit()?,
        0,
    )
    .await?;

    Ok(())
}
