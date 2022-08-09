//! command login
use crate::{metadata::Metadata, Result};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum Action {
    /// Display the structure of the metadata
    Display,
}

/// Login to account
#[derive(Debug, StructOpt)]
pub struct Meta {
    /// Path of "*.meta.wasm".
    pub metadata: PathBuf,
    #[structopt(subcommand)]
    pub action: Action,
}

impl Meta {
    /// exec command login
    pub fn exec(&self) -> Result<()> {
        let wasm = fs::read(&self.metadata)?;
        let meta = Metadata::of(&wasm)?;

        match self.action {
            Action::Display => {
                println!("{}", format!("{:#}", meta).replace('"', ""));
            }
        }

        Ok(())
    }
}
