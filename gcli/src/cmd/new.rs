// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use crate::{result::Result, template};
use clap::Parser;

/// Create a new gear program
#[derive(Debug, Parser)]
pub struct New {
    /// Create gear program from templates
    pub template: Option<String>,
}

impl New {
    /// run command new
    pub async fn exec(&self) -> Result<()> {
        let templates = template::list().await?;

        if let Some(template) = &self.template {
            if templates.contains(template) {
                template::download(template).await?;
                println!("Successfully created {template}!");
            } else {
                println!("Available templates: {:#?}", templates);
            }
        } else {
            template::download("app").await?;
            println!("Successfully created app!");
        }

        Ok(())
    }
}
