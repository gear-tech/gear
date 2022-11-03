#![cfg(feature = "bin")]
use common::env;
use gear_program::{
    api::Api,
    result::{ClientError, Error},
};
use std::path::PathBuf;

mod cmd;
mod common;

#[tokio::test]
async fn api_timeout() {
    assert!(matches!(
        Api::new_with_timeout(None, Some(10)).await.err(),
        Some(Error::Client(ClientError::SubxtRpc(
            jsonrpsee::core::Error::Transport(..)
        )))
    ));
}

#[test]
fn paths() {
    assert!(PathBuf::from(env::bin("gear")).exists());
    assert!(PathBuf::from(env::bin("gprogram")).exists());
}
