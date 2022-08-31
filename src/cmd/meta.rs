//! command `meta`
use crate::{metadata::Metadata, result::Result};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, StructOpt)]
pub enum Action {
    /// Display the structure of the metadata.
    Display,
}

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, StructOpt)]
pub struct Meta {
    /// Path of "*.meta.wasm".
    pub metadata: PathBuf,
    #[structopt(subcommand)]
    pub action: Action,
}

impl Meta {
    /// Run command meta.
    pub fn exec(&self) -> Result<()> {
        let wasm = fs::read(&self.metadata)?;
        let meta = Metadata::of(&wasm)?;

        match self.action {
            Action::Display => println!("{}", format!("{:#}", meta).replace('"', "")),
        }

        Ok(())
    }
}
