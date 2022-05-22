//! command submit
use crate::{
    api::{generated::api::gear::calls::SubmitProgram, Api},
    Result,
};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Deploy program to gear node
#[derive(StructOpt, Debug)]
pub struct Deploy {
    /// gear node rpc endpoint
    #[structopt(short, long)]
    endpoint: Option<String>,
    /// gear program code <*.wasm>
    code: PathBuf,
    /// gear program salt ( hex encoding )
    #[structopt(default_value = "0x00")]
    salt: String,
    /// gear program init payload ( hex encoding )
    #[structopt(default_value = "0x00")]
    init_payload: String,
    /// gear program gas limit
    ///
    /// if zero, gear will estimate this automatically
    #[structopt(default_value = "0")]
    gas_limit: u64,
    /// gear program balance
    #[structopt(default_value = "0")]
    value: u128,
    /// password of the signer account
    #[structopt(short, long)]
    passwd: Option<String>,
}

impl Deploy {
    /// exec command submit
    pub async fn exec(&self) -> Result<()> {
        let api = Api::new(
            self.endpoint.as_ref().map(|s| s.as_ref()),
            self.passwd.as_ref().map(|s| s.as_ref()),
        )
        .await?;

        // estimate gas
        let gas_limit = api
            .estimate_gas(self.gas_limit, || async {
                api.get_init_gas_spent(
                    fs::read(&self.code)?.into(),
                    hex::decode(&self.init_payload.trim_start_matches("0x"))?.into(),
                    0,
                    None,
                )
                .await
            })
            .await?;

        // submit program
        api.submit_program(SubmitProgram {
            code: fs::read(&self.code)?,
            salt: hex::decode(&self.salt.trim_start_matches("0x"))?,
            init_payload: hex::decode(&self.init_payload.trim_start_matches("0x"))?,
            gas_limit,
            value: self.value,
        })
        .await?;

        Ok(())
    }
}
