// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

fn main() {
    #[cfg(not(any(windows, target_os = "cygwin")))]
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        use rustc_version::{Version, VersionMeta, version_meta};

        let VersionMeta {
            semver: Version { major, minor, .. },
            commit_date,
            ..
        } = version_meta().expect("failed to get rustc version");

        if major >= 1 && minor >= 89 && commit_date != Some("2025-06-05".into()) {
            panic!(
                "Rust >= 1.89 is not supported, use Rust 1.88: https://github.com/wasmerio/wasmer/issues/5610"
            );
        }
    }
}
