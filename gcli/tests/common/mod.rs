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
    args::Args,
    node::Node,
    result::{Error, Result},
};
use gear_core::ids::{CodeId, ProgramId};
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};
use std::{
    iter::IntoIterator,
    process::{Command, Output},
};

mod args;
pub mod env;
pub mod logs;
mod node;
mod port;
mod result;
pub mod traits;

#[cfg(not(feature = "vara-testing"))]
mod prelude {
    pub use scale_info::scale::Encode;

    pub const ALICE_SS58_ADDRESS: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    pub const MESSAGER_SENT_VALUE: u128 = 5_000;
}

#[cfg(not(feature = "vara-testing"))]
pub use prelude::*;

// TODO: refactor this implementation after #2481.
impl Node {
    /// Run binary `gcli`
    pub fn run(&self, args: Args) -> Result<Output> {
        gcli(Vec::<String>::from(args.endpoint(self.ws())))
    }
}

/// Run binary `gcli`
pub fn gcli<T: ToString>(args: impl IntoIterator<Item = T>) -> Result<Output> {
    Ok(Command::new(env::bin("gcli"))
        .args(
            args.into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>(),
        )
        .output()?)
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

    let args = Args::new("upload").program(env::wasm_bin("messager.opt.wasm"));
    #[cfg(not(feature = "vara-testing"))]
    let args = args
        .payload("0x".to_owned() + &hex::encode(MESSAGER_SENT_VALUE.encode()))
        .value("10000000");

    let _ = node.run(args)?;
    Ok(node)
}
