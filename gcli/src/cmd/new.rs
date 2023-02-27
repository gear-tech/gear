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

//! command `new`
use crate::result::Result;
use clap::Parser;
use std::process::{self, Command};

const ORG: &str = "https://github.com/gear-dapps/";
const GIT_SUFFIX: &str = ".git";
const TEMPLATES: &[&str] = &[
    "concert",
    "crowdsale-ico",
    "dao",
    "dao-light",
    "dutch-auction",
    "escrow",
    "feeds",
    "fungible-token",
    "gear-feeds-channel",
    "lottery",
    "multisig-wallet",
    "nft-pixelboard",
    "non-fungible-token",
    "ping",
    "RMRK",
    "rock-paper-scissors",
    "staking",
    "supply-chain",
    "swap",
];

/// Create a new gear program
#[derive(Debug, Parser)]
pub struct New {
    /// Create gear program from templates
    pub template: Option<String>,
}

impl New {
    fn template(name: &str) -> String {
        ORG.to_string() + name + GIT_SUFFIX
    }

    fn help() {
        println!("Available templates: \n\n{}", TEMPLATES.join("\n"));
    }

    /// run command new
    pub async fn exec(&self) -> Result<()> {
        if let Some(template) = &self.template {
            if TEMPLATES.contains(&template.as_ref()) {
                if !Command::new("git")
                    .args(["clone", &Self::template(template)])
                    .status()?
                    .success()
                {
                    process::exit(1);
                }
            } else {
                crate::template::create(template)?;
            }

            println!("Successfully created {template}!");
        } else {
            Self::help();
        }

        Ok(())
    }
}
