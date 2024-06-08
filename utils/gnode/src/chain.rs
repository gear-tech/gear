// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Node arguments

const DEFAULT_ARGS: [&str; 2] = ["--tmp", "--no-hardware-benchmarks"];

/// Arguments of the node instance, current just support `--dev` or `--vara-dev`
/// since the node instance should only be used in development at the moment.
#[derive(Default)]
pub enum Chain {
    /// `--dev` argument for the node instance
    #[default]
    Dev,
    /// `--chain=vara-dev` argument for the node instance
    VaraDev,
}

impl AsRef<str> for Chain {
    fn as_ref(&self) -> &str {
        match self {
            Self::Dev => "--dev",
            Self::VaraDev => "--chain=vara-dev",
        }
    }
}

impl Chain {
    /// Convert self to node arguments
    pub(crate) fn to_args<'a>(&'a self, port: &'a str) -> Vec<&str> {
        let mut args = vec![self.as_ref(), "--rpc-port", port];
        args.extend_from_slice(&DEFAULT_ARGS);
        args
    }
}
