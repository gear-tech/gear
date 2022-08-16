use crate::result::Result;
use std::{fs, process::Command};

const NAME: &str = "$NAME";
const USER: &str = "$USER";

// cargo.toml
pub const CARTO_TOML: &str = r#"
[package]
name = "$NAME"
version = "0.1.0"
authors = ["$USER"]
edition = "2021"
license = "GPL-3.0"

[dependencies]
gstd = { git = "https://github.com/gear-tech/gear.git", features = ["debug"] }

[build-dependencies]
gear-wasm-builder = { git = "https://github.com/gear-tech/gear.git" }

[dev-dependencies]
gtest = { git = "https://github.com/gear-tech/gear.git" }
"#;

// build.rs
pub const BUILD_RS: &str = r#"
fn main() {
    gear_wasm_builder::build();
}
"#;

// lib.rs
pub const LIB_RS: &str = r#"
#![no_std]

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid message");

    if new_msg == "PING" {
        msg::reply_bytes("PONG", 0).unwrap();
    }

    MESSAGE_LOG.push(new_msg);

    debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

}
"#;

/// create gear program
pub fn create(name: &str) -> Result<()> {
    let user_bytes = Command::new("git")
        .args(&["config", "--global", "--get", "user.name"])
        .output()?
        .stdout;
    let user = String::from_utf8_lossy(&user_bytes);

    let email_bytes = Command::new("git")
        .args(&["config", "--global", "--get", "user.email"])
        .output()?
        .stdout;
    let email = String::from_utf8_lossy(&email_bytes);

    fs::create_dir_all(format!("{}/src", name))?;

    fs::write(
        format!("{}/Cargo.toml", name),
        CARTO_TOML
            .replace(NAME, name)
            .replace(USER, &format!("{} <{}>", user.trim(), email.trim())),
    )?;
    fs::write(format!("{}/build.rs", name), BUILD_RS)?;
    fs::write(format!("{}/src/lib.rs", name), LIB_RS)?;

    Ok(())
}
