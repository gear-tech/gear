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

//! Handlers for specified manifest.

/// gear-core-processor handler.
pub mod core_processor {
    use toml_edit::{Document, InlineTable};

    /// Rename core processor related package in the
    /// manifest of workspace since `gear-core-processor`
    /// has been published by others.
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
            "core-processor" => {
                table.remove("package");
            }
            "gear-core-processor" => {
                table.insert("package", "core-processor".into());
            }
            _ => {}
        }
    }

    /// Patch the manifest of core-processor.
    pub fn patch(manifest: &mut Document) {
        manifest["package"]["name"] = toml_edit::value("core-processor");
    }
}

/// substrate handler.
pub mod substrate {
    use toml_edit::InlineTable;

    /// Patch the substrate packages in the manifest of workspace.
    ///
    /// NOTE: The packages inside of this function are located at
    /// <https://github.com/gear-tech/substrate/tree/cl/1.0.3-crates-io>.
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
            // sp-allocator is outdated on crates.io, last
            // 3.0.0 forever, here we use gp-allocator instead.
            "sp-allocator" => {
                table.insert("version", "4.1.1".into());
                table.insert("package", "gp-allocator".into());
            }
            // Our sp-wasm-interface is different from the
            // original one.
            "sp-wasm-interface" => {
                table.insert("package", "gp-wasm-interface".into());
                table.insert("version", "7.0.1".into());
            }
            // Related to sp-wasm-interface.
            "sp-wasm-interface-common" => {
                table.insert("version", "7.0.1".into());
            }
            // Related to sp-wasm-interface.
            "sp-runtime-interface" => {
                table.insert("version", "7.0.3".into());
                table.insert("package", "gp-runtime-interface".into());
            }
            // The versions of these packages on crates.io are incorrect.
            "sp-arithmetic" | "sp-core" | "sp-rpc" | "sp-version" => {
                table.insert("version", "7.0.0".into());
            }
            // Filter out this package for local testing.
            "frame-support-test" => return,
            _ => {}
        }

        table.remove("branch");
        table.remove("git");
    }
}

/// wasmi handler.
pub mod wasmi {
    use toml_edit::InlineTable;

    /// Convert the wasmi module to the crates-io version.
    pub fn patch_workspace(table: &mut InlineTable) {
        table.remove("branch");
        table.remove("git");
    }
}
