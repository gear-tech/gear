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
use colored::Colorize;
use std::path::PathBuf;

/// Create a new project of Gear program from a template.
#[derive(Clone, Debug, Parser)]
pub struct New {
    /// List all available templates and do nothing.
    #[arg(short, long)]
    list: bool,

    /// Template name.
    #[arg(short, long, default_value = "dapp-template")]
    template: String,

    /// Path to create project at, defaults to the name of the template.
    path: Option<PathBuf>,
}

impl New {
    pub async fn exec(self) -> Result<()> {
        let templates = template::list().await?;
        let list_templates = || {
            println!("{}", "Available templates:".bold());
            for template in &templates {
                println!("- {template}")
            }
        };

        if self.list {
            list_templates();
            return Ok(());
        }

        if templates.contains(&self.template) {
            let path = self
                .path
                .as_deref()
                .unwrap_or_else(|| self.template.as_ref());
            template::download(&self.template, path).await?;
            println!(
                "Successfully created `{}`",
                path.display().to_string().blue()
            );
        } else {
            println!("Template `{}` is not found", self.template.purple());
            println!();
            list_templates();
        }

        Ok(())
    }
}
