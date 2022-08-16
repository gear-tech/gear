//! command update
use crate::{registry, result::Result};
use std::process::{self, Command};
use structopt::StructOpt;

/// Update resources
#[derive(Debug, StructOpt)]
pub struct Update {
    /// Update gear examples
    #[structopt(short, long)]
    pub examples: bool,
    /// Update self
    #[structopt(short, long)]
    pub gear: bool,
}

impl Update {
    /// update self
    async fn update_self(&self) -> Result<()> {
        if !Command::new("cargo")
            .args(&["install", "gear-program"])
            .status()?
            .success()
        {
            process::exit(1);
        }

        Ok(())
    }

    /// update examples
    async fn update_examples(&self) -> Result<()> {
        registry::update().await?;

        Ok(())
    }

    /// exec command update
    pub async fn exec(&self) -> Result<()> {
        registry::init().await?;
        if self.gear {
            self.update_self().await?;
        }

        if self.examples {
            self.update_examples().await?;
        }

        Ok(())
    }
}
