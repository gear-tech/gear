// This file is part of Gear.
//
// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use gear_node_wrapper::{Node, NodeInstance};
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

pub fn dev_node() -> NodeInstance {
    // Use release build because of performance reasons.
    let bin_path = env!("CARGO_MANIFEST_DIR").to_owned() + "/../target/release/gear";

    Node::from_path(bin_path)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
        .spawn()
        .expect("Failed to spawn node process")
}

pub fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap()
}
