//! command submit
use crate::{
    api::{generated::api::gear::calls::SubmitProgram, Api},
    Result,
};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Submit program to gear node
#[derive(StructOpt, Debug)]
pub struct Submit {
    /// gear node rpc endpoint
    #[structopt(short, long)]
    endpoint: Option<String>,
    /// gear program code <*.wasm>
    code: PathBuf,
    /// gear program salt ( hex encoding )
    salt: String,
    /// gear program init payload ( hex encoding )
    init_payload: String,
    /// gear program gas limit
    gas_limit: u64,
    /// gear program balance
    value: u128,
}

impl Submit {
    /// exec command submit
    pub async fn exec(&self) -> Result<()> {
        let api = Api::new(self.endpoint.as_ref().map(|s| s.as_ref())).await?;

        // params
        let code = fs::read(&self.code)?;
        let salt = hex::decode(&self.salt)?;
        let init_payload = hex::decode(&self.init_payload)?;
        let gas_limit = self.gas_limit;
        let value = self.value;

        api.submit_program(SubmitProgram {
            code,
            salt,
            init_payload,
            gas_limit,
            value,
        })
        .await?;

        Ok(())
    }
}
