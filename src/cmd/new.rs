//! command new
use crate::result::Result;
use std::process::{self, Command};
use structopt::StructOpt;

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
#[derive(Debug, StructOpt)]
pub struct New {
    /// Create gear program from templates
    pub template: Option<String>,
}

impl New {
    fn template(name: &str) -> String {
        ORG.to_string() + name + GIT_SUFFIX
    }

    fn help() {
        println!("Available templates:\n\t{}", TEMPLATES.join("\n\t"));
    }

    /// run command new
    pub async fn exec(&self) -> Result<()> {
        if let Some(template) = &self.template {
            if TEMPLATES.contains(&template.as_ref()) {
                if !Command::new("git")
                    .args(&["clone", &Self::template(template)])
                    .status()?
                    .success()
                {
                    process::exit(1);
                }
            } else {
                crate::template::create(template)?;
            }

            println!("Successfully created {}!", template);
        } else {
            Self::help();
        }

        Ok(())
    }
}
