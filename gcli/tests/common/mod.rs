// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Common utils for integration tests
pub use self::{
    node::Node,
    result::{Error, Result},
};
use gear_core::ids::{CodeId, ProgramId};
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};
use std::process::{Command, Output};

pub mod env;
pub mod logs;
mod node;
mod port;
mod result;
pub mod traits;

pub const ALICE_SS58_ADDRESS: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

/// Run binary `gear`
pub fn gear(args: &[&str]) -> Result<Output> {
    Ok(Command::new(env::bin("gcli")).args(args).output()?)
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
pub fn program_id(bin: &[u8], salt: &[u8]) -> ProgramId {
    ProgramId::generate(CodeId::generate(bin), salt)
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

    let messager = env::wasm_bin("messager.opt.wasm");
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
