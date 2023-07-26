// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{borrow::Cow, process::Command};

fn main() {
    substrate_build_script_utils::generate_cargo_keys();
    let git_commit_hash = || -> Cow<str> {
        // This code is taken from
        // https://github.com/paritytech/substrate/blob/ae1a608c91a5da441a0ee7c26a4d5d410713580d/utils/build-script-utils/src/version.rs#L21
        let commit = if let Ok(hash) = std::env::var("SUBSTRATE_CLI_GIT_COMMIT_HASH") {
            Cow::from(hash.trim().to_owned())
        } else {
            // We deliberately set the length here to `11` to ensure that
            // the emitted hash is always of the same length; otherwise
            // it can (and will!) vary between different build environments.
            match Command::new("git")
                .args(["rev-parse", "--short=11", "HEAD"])
                .output()
            {
                Ok(o) if o.status.success() => {
                    let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
                    Cow::from(sha)
                }
                Ok(o) => {
                    println!("cargo:warning=Git command failed with status: {}", o.status);
                    Cow::from("unknown")
                }
                Err(err) => {
                    println!("cargo:warning=Failed to execute git command: {}", err);
                    Cow::from("unknown")
                }
            }
        };
        commit
    };
    println!("cargo:warning=GIT_COMMIT_HASH={}", git_commit_hash());
    #[cfg(all(feature = "std", not(feature = "fuzz")))]
    {
        substrate_wasm_builder::WasmBuilder::new()
            .with_current_project()
            .export_heap_base()
            .import_memory()
            .build()
    }
}
