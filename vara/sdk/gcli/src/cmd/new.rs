// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
