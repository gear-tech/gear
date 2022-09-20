#![cfg(feature = "cli")]
use common::Result;
use gear_program::{api::Api, result::Error};
use std::fs::File;

mod cmd;
mod common;

#[test]
fn check_spec_version() -> Result<()> {
    use common::spec_version::{self, GEAR_NODE_SPEC_VERSION_PATTERN};

    let mut node = common::Node::dev()?;
    let line = node.wait(GEAR_NODE_SPEC_VERSION_PATTERN)?;

    let current_version =
        spec_version::find(&File::open("src/api/generated.rs").expect("generated.rs not found."))
            .expect("Failed to parse spec_version from generated.rs");

    let latest_version: u16 =
        spec_version::parse(&line).expect("Failed to parse spec version from log.");

    assert_eq!(current_version, latest_version);

    Ok(())
}

#[tokio::test]
async fn api_timeout() {
    assert!(matches!(
        Api::new_with_timeout(None, Some(10)).await.err(),
        Some(Error::Ws(
            jsonrpsee_client_transport::ws::WsHandshakeError::Timeout(..)
        ))
    ));
}
