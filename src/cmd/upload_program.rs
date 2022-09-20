//! command `upload_program`
use crate::{api::signer::Signer, result::Result, utils};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Deploy program to gear node
#[derive(StructOpt, Debug)]
pub struct UploadProgram {
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

impl UploadProgram {
    /// Exec command submit
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let code = fs::read(&self.code)?;
        let payload = utils::hex_to_vec(&self.init_payload)?;

        let gas = if self.gas_limit == 0 {
            signer
                .calculate_upload_gas(code.clone(), payload.clone(), self.value, false, None)
                .await?
                .min_limit
        } else {
            self.gas_limit
        };

        // estimate gas
        let gas_limit = signer.cmp_gas_limit(gas).await?;

        // upload program
        signer
            .upload_program(
                code,
                utils::hex_to_vec(&self.salt)?,
                payload,
                gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
