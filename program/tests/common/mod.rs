//! Common utils for integration tests
pub use self::{
    node::Node,
    result::{Error, Result},
};
use blake2_rfc::blake2b;
use std::{
    path::PathBuf,
    process::{Command, Output},
};
use subxt::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

pub mod env;
pub mod logs;
mod node;
mod port;
mod result;
// pub mod spec_version;
pub mod traits;

const WASM_TARGET: &str = "target/wasm32-unknown-unknown/";
pub const ALICE_SS58_ADDRESS: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

/// Run binary `gear`
pub fn gear(args: &[&str]) -> Result<Output> {
    Ok(Command::new(env::bin("gear")).args(args).output()?)
}

/// Creates a unique identifier by passing given argument to blake2b hash-function.
pub fn hash(argument: &[u8]) -> [u8; 32] {
    let mut arr: [u8; 32] = Default::default();

    let blake2b_hash = blake2b::blake2b(32, &[], argument);
    arr[..].copy_from_slice(blake2b_hash.as_bytes());

    arr
}

/// Init env logger
#[allow(dead_code)]
pub fn init_logger() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Login as //Alice
pub fn login_as_alice() -> Result<()> {
    let _ = gear(&["login", "//Alice"])?;

    Ok(())
}

/// Generate program id from code id and salt
pub fn program_id(bin: &[u8], salt: &[u8]) -> [u8; 32] {
    let code_id = hash(bin);
    let len = code_id.len() + salt.len();

    let mut argument = Vec::with_capacity(len);
    argument.extend_from_slice(&code_id);
    argument.extend_from_slice(salt);

    hash(&argument)
}

/// Get wasm binary path
pub fn wasm_path(name: &str) -> String {
    [
        WASM_TARGET,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        "/",
        name,
        ".opt.wasm",
    ]
    .concat()
}

/// AccountId32 of `addr`
pub fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")
        .expect("Invalid address")
}

/// Create program messager
pub async fn create_messager() -> Result<Node> {
    login_as_alice()?;
    let mut node = Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let messager = wasm_path("messager");
    let _ = gear(&[
        "-e",
        &node.ws(),
        "upload-program",
        &messager,
        "0x",
        "0x",
        "0",
        "10000000000",
    ])?;

    Ok(node)
}
