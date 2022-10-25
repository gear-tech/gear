//! command `meta`
use crate::{metadata::Metadata, result::Result};
use clap::Parser;
use std::{fs, path::PathBuf};

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, Parser)]
pub enum Action {
    /// Display the structure of the metadata.
    Display,
}

/// Show metadata structure, read types from registry, etc.
#[derive(Debug, Parser)]
pub struct Meta {
    /// Path of "*.meta.wasm".
    pub metadata: PathBuf,
    #[clap(subcommand)]
    pub action: Action,
}

impl Meta {
    /// Run command meta.
    pub fn exec(&self) -> Result<()> {
        let wasm = fs::read(&self.metadata)?;
        let meta = Metadata::of(&wasm)?;

        match self.action {
            Action::Display => println!("{}", format!("{meta:#}").replace('"', "")),
        }

        Ok(())
    }
}
