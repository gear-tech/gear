//! Gear program template

use crate::result::Result;
use std::{fs, process::Command};

const NAME: &str = "$NAME";
const USER: &str = "$USER";

/// cargo.toml
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

/// build.rs
pub const BUILD_RS: &str = r#"
fn main() {
    gear_wasm_builder::build();
}
"#;

/// lib.rs
pub const LIB_RS: &str = r#"
#![no_std]

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load_bytes().unwrap()).expect("Invalid message");

    if new_msg == "PING" {
        msg::reply_bytes("PONG", 0).unwrap();
    }

    unsafe { MESSAGE_LOG.push(new_msg) };

    debug!("{:?} total message(s) stored: ", unsafe { MESSAGE_LOG.len() });
}
"#;

/// create rust project for gear program in `PWD`
pub fn create(name: &str) -> Result<()> {
    let user_bytes = Command::new("git")
        .args(["config", "--global", "--get", "user.name"])
        .output()?
        .stdout;
    let user = String::from_utf8_lossy(&user_bytes);

    let email_bytes = Command::new("git")
        .args(["config", "--global", "--get", "user.email"])
        .output()?
        .stdout;
    let email = String::from_utf8_lossy(&email_bytes);

    fs::create_dir_all(format!("{name}/src"))?;

    fs::write(
        format!("{name}/Cargo.toml"),
        CARTO_TOML
            .replace(NAME, name)
            .replace(USER, &format!("{} <{}>", user.trim(), email.trim())),
    )?;
    fs::write(format!("{name}/build.rs"), BUILD_RS)?;
    fs::write(format!("{name}/src/lib.rs"), LIB_RS)?;

    Ok(())
}
