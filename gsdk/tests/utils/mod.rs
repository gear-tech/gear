// This file is part of Gear.
//
// Copyright (C) 2023-2025 Gear Technologies Inc.
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
use sp_core::crypto::Ss58Codec;
use sp_runtime::AccountId32;
use std::{env, env::consts::EXE_EXTENSION, path::PathBuf};

pub fn dev_node() -> NodeInstance {
    // Use release build because of performance reasons.
    let bin_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut bin_path = bin_path.join("../target/release/gear");
    bin_path.set_extension(EXE_EXTENSION);

    Node::from_path(bin_path)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
        .spawn()
        .expect("Failed to spawn node process")
}

pub fn alice_account_id() -> AccountId32 {
    sp_runtime::AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")
        .unwrap()
}
