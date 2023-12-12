// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    args::Args,
    node::{Convert, NodeExec},
    result::{Error, Result},
};
use gear_core::ids::{CodeId, ProgramId};
use gsdk::{
    ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32},
    testing::Node,
};
pub use scale_info::scale::Encode;
use std::{
    iter::IntoIterator,
    process::{Command, Output},
};

mod args;
pub mod env;
pub mod node;
mod result;

pub const ALICE_SS58_ADDRESS: &str = "kGkLEU3e3XXkJp2WK4eNpVmSab5xUNL9QtmLPh8QfCL2EgotW";

impl NodeExec for Node {
    fn ws(&self) -> String {
        "ws://".to_string() + &self.address().to_string()
    }

    /// Run binary `gcli`
    fn run(&self, args: Args) -> Result<Output> {
        gcli(Vec::<String>::from(args.endpoint(self.ws())))
    }
}

/// Run binary `gcli`
pub fn gcli<T: ToString>(args: impl IntoIterator<Item = T>) -> Result<Output> {
    Command::new(env::bin("gcli"))
        .args(
            args.into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>(),
        )
        .output()
        .map_err(Into::into)
}

/// Run the dev node
pub fn dev() -> Result<Node> {
    login_as_alice()?;

    let args = vec!["--tmp", "--dev"];
    let mut node = Node::try_from_path(env::bin("gear"), args)?;

    // TODO: use [`Node::wait_while_initialized`] instead,
    // it currently presents infinite loop even after capturing
    // the specified log #3304.
    node.wait_for_log_record("Imported #1")?;
    Ok(node)
}

/// Init env logger
#[allow(dead_code)]
pub fn init_logger() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Login as //Alice
pub fn login_as_alice() -> Result<()> {
    let _ = gcli(["login", "//Alice"])?;

    Ok(())
}

/// Generate program id from code id and salt
pub fn program_id(bin: &[u8], salt: &[u8]) -> ProgramId {
    ProgramId::generate_from_user(CodeId::generate(bin), salt)
}

/// AccountId32 of `addr`
pub fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check("kGkLEU3e3XXkJp2WK4eNpVmSab5xUNL9QtmLPh8QfCL2EgotW")
        .expect("Invalid address")
}

/// Create program messager
pub async fn create_messager() -> Result<Node> {
    let node = dev()?;

    let args = Args::new("upload").program(env::wasm_bin("demo_messager.opt.wasm"));
    let _ = node.run(args)?;

    Ok(node)
}
