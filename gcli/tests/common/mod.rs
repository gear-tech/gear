// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
pub use self::{args::Args, node::NodeExec, result::Result};
use gear_core::ids::{prelude::*, CodeId, ProgramId};
use gear_node_wrapper::{Node, NodeInstance};
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};
use std::{
    iter::IntoIterator,
    process::{Command, Output},
};

mod app;
mod args;
pub mod env;
pub mod node;
mod result;

pub const ALICE_SS58_ADDRESS: &str = "kGkLEU3e3XXkJp2WK4eNpVmSab5xUNL9QtmLPh8QfCL2EgotW";
pub const RENT_POOL_SS58_ADDRESS: &str = "kGkkENXuYL4Xw6H1ymWm6VwHLi66s56Ywt45pf9hEx1hmx5MV";

impl NodeExec for NodeInstance {
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
pub fn dev() -> Result<NodeInstance> {
    login_as_alice()?;
    Node::from_path(env::node_bin())?
        .spawn()
        .map_err(Into::into)
}

/// Init env logger
#[allow(dead_code)]
pub fn init_logger() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Login as //Alice
pub fn login_as_alice() -> Result<()> {
    let _ = gcli(["wallet", "dev"])?;

    Ok(())
}

/// Generate program id from code id and salt
pub fn program_id(bin: &[u8], salt: &[u8]) -> ProgramId {
    ProgramId::generate_from_user(CodeId::generate(bin), salt)
}

/// AccountId32 of `addr`
pub fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check(ALICE_SS58_ADDRESS).expect("Invalid address")
}

/// Create program messager
pub async fn create_messager() -> Result<NodeInstance> {
    let node = dev()?;

    let args = Args::new("upload").program(env::wasm_bin("demo_messenger.opt.wasm"));
    let _ = node.run(args)?;

    Ok(node)
}
