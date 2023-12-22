// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

use gsdk::{
    ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32},
    testing::Node,
};
use std::env;

pub fn dev_node() -> Node {
    // Use release or CI profile because of performance reasons.
    let profile = env::var("CI").map(|_| "ci").unwrap_or_else(|_| "release");
    let bin_path = format!("{}/../target/{}/gear", env!("CARGO_MANIFEST_DIR"), profile);

    let args = vec!["--tmp", "--dev"];

    Node::try_from_path(bin_path, args)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
}

pub fn node_uri(node: &Node) -> String {
    format!("ws://{}", &node.address())
}

pub fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap()
}
