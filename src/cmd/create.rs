//! command submit
use crate::{
    api::{
        generated::api::gear::{calls::UploadProgram, Event as GearEvent},
        signer::Signer,
        Api,
    },
    result::Result,
};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Deploy program to gear node
#[derive(StructOpt, Debug)]
pub struct Create {
    /// gear program code <*.wasm>
    code: PathBuf,
    /// gear program salt ( hex encoding )
    #[structopt(default_value = "0x")]
    salt: String,
    /// gear program init payload ( hex encoding )
    #[structopt(default_value = "0x")]
    init_payload: String,
    /// gear program gas limit
    ///
    /// if zero, gear will estimate this automatically
    #[structopt(default_value = "0")]
    gas_limit: u64,
    /// gear program balance
    #[structopt(default_value = "0")]
    value: u128,
}

impl Create {
    /// Exec command submit
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let events = signer.events().await?;

        tokio::try_join!(
            self.submit_program(&signer),
            Api::wait_for(events, |event| {
                matches!(event, GearEvent::MessageEnqueued { .. })
            })
        )?;

        Ok(())
    }

    async fn submit_program(&self, api: &Signer) -> Result<()> {
        let gas = if self.gas_limit == 0 {
            api.get_init_gas_spent(
                fs::read(&self.code)?.into(),
                hex::decode(&self.init_payload.trim_start_matches("0x"))?.into(),
                0,
                None,
            )
            .await?
            .min_limit
        } else {
            self.gas_limit
        };

        // estimate gas
        let gas_limit = api.cmp_gas_limit(gas).await?;

        // submit program
        api.submit_program(UploadProgram {
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
