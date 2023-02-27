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

//! command `meta`
use crate::{metadata::Metadata, result::Result};
use clap::Parser;
use std::{fs, path::PathBuf};

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, Parser)]
pub enum Action {
    /// Display the structure of the metadata.
    Display,
}

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, Parser)]
pub struct Meta {
    /// Path of "*.meta.wasm".
    pub metadata: PathBuf,
    #[command(subcommand)]
    pub action: Action,
}

impl Meta {
    /// Run command meta.
    pub fn exec(&self) -> Result<()> {
        let wasm = fs::read(&self.metadata)?;
        let meta = Metadata::of(&wasm)?;

        match self.action {
            Action::Display => println!("{}", format!("{meta:#}").replace('"', "")),
        }

        Ok(())
    }
}
