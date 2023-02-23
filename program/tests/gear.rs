#![cfg(feature = "bin")]
use common::env;
use gsdk::{result::Error, Api};
use std::path::PathBuf;

mod cmd;
mod common;
mod rpc;

#[tokio::test]
async fn api_timeout() {
    assert!(matches!(
        Api::new_with_timeout(None, Some(10)).await.err(),
        Some(Error::SubxtRpc(jsonrpsee::core::Error::Transport(..)))
    ));
}

#[test]
fn paths() {
    assert!(PathBuf::from(env::bin("gear")).exists());
    assert!(PathBuf::from(env::bin("gprogram")).exists());
    assert!(PathBuf::from(env::wasm_bin("demo_meta.opt.wasm")).exists());
    assert!(PathBuf::from(env::wasm_bin("demo_meta.meta.wasm")).exists());
}
