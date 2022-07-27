//! command transfer
use crate::{api::Api, Result};
use structopt::StructOpt;

/// Get account info from ss58address.
#[derive(Debug, StructOpt)]
pub struct Info {
    /// Get info of this address (ss58address).
    address: String,
}

impl Info {
    /// execute command transfer
    pub async fn exec(&self, api: Api) -> Result<()> {
        let info = api.info(&self.address).await?;

        println!("{info:#?}");

        Ok(())
    }
}
