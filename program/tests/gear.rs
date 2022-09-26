#![cfg(feature = "cli")]
use common::Result;
use gear_program::{api::Api, result::Error};
use std::fs::File;

mod cmd;
mod common;

#[tokio::test]
async fn api_timeout() {
    assert!(matches!(
        Api::new_with_timeout(None, Some(10)).await.err(),
        Some(Error::Ws(
            jsonrpsee_client_transport::ws::WsHandshakeError::Timeout(..)
        ))
    ));
}
