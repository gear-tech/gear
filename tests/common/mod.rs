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

mod docker;
pub mod logs;
mod node;
mod result;
pub mod spec_version;
pub mod traits;

/// Run binary `gear`
pub fn gear(args: &[&str]) -> Result<Output> {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    Ok(
        Command::new(PathBuf::from("target/".to_owned() + profile + "/gear"))
            .args(args)
            .output()?,
    )
}

/// Creates a unique identifier by passing given argument to blake2b hash-function.
fn hash(argument: &[u8]) -> [u8; 32] {
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

    hash(&argument).into()
}
