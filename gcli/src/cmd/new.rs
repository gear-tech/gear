// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! command `new`
use crate::template;
use anyhow::Result;
use clap::Parser;

/// Create a new gear program
#[derive(Clone, Debug, Parser)]
pub struct New {
    /// Create gear program from templates.
    #[arg(short, long, default_value = "dapp-template")]
    pub template: String,

    /// Create gear program in specified path.
    pub path: Option<String>,
}

impl New {
    /// run command new
    pub async fn exec(&self) -> Result<()> {
        let templates = template::list().await?;

        let template = &self.template;
        if templates.contains(template) {
            let path = self.path.as_deref().unwrap_or(template);
            template::download(template, path).await?;
            println!("Successfully created {path}!");
        } else {
            println!("Template not found, available templates: {templates:#?}");
        }

        Ok(())
    }
}
