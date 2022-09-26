//! Command `program`.
use crate::{
    api::Api,
    metadata::Metadata,
    result::{Error, Result},
    utils,
};
use clap::Parser;
use std::{fs, path::PathBuf};
use subxt::sp_core::H256;

/// Read program state, etc.
#[derive(Clone, Debug, Parser)]
pub enum Action {
    /// Read program state.
    State {
        /// Path of "*.meta.wasm".
        metadata: PathBuf,
        /// Input message for reading program state.
        #[clap(short, long, default_value = "0x")]
        msg: String,
        /// Block timestamp.
        #[clap(short, long)]
        timestamp: Option<u64>,
        /// Block height.
        #[clap(short, long)]
        height: Option<u64>,
    },
}

/// Read program state, etc.
#[derive(Debug, Parser)]
pub struct Program {
    /// Program id.
    pid: String,
    #[clap(subcommand)]
    action: Action,
}

impl Program {
    /// Run command program.
    pub async fn exec(&self, api: Api) -> Result<()> {
        let pid_bytes = hex::decode(self.pid.trim_start_matches("0x"))?;
        let mut pid = [0; 32];
        pid.copy_from_slice(&pid_bytes);

        match self.action {
            Action::State { .. } => self.state(api, pid.into()).await?,
        }

        Ok(())
    }

    /// Read program state.
    pub async fn state(&self, api: Api, pid: H256) -> Result<()> {
        let Action::State {
            metadata,
            msg,
            timestamp,
            height,
        } = self.action.clone();

        // Get program
        let program = api.gprog(pid).await?;
        let code_id = program.code_hash;
        let code = api
            .code_storage(code_id.0)
            .await?
            .ok_or_else(|| Error::CodeNotFound(self.pid.clone()))?;
        let pages = api.gpages(pid, program).await?;

        // Query state
        let state = Metadata::read(
            &fs::read(&metadata)?,
            code.static_pages.0 as u64,
            pages,
            utils::hex_to_vec(msg)?,
            timestamp.unwrap_or(0),
            height.unwrap_or(0),
        )?;

        println!("state: 0x{}", hex::encode(state));

        Ok(())
    }
}
