//! command update
use crate::{Registry, Result};
use std::process::Command;
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
        Command::new("cargo")
            .args(&["install", "gear-program"])
            .status()?;

        Ok(())
    }

    /// update examples
    async fn update_examples(&self) -> Result<()> {
        Registry::default().update().await?;

        Ok(())
    }

    /// exec command update
    pub async fn exec(&self) -> Result<()> {
        if self.gear {
            self.update_self().await?;
        }

        if self.examples {
            self.update_examples().await?;
        }

        Ok(())
    }
}
