//! command transfer
use crate::{api::signer::Signer, result::Result};
use structopt::StructOpt;
use subxt::sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT};

/// Get account info from ss58address.
#[derive(Debug, StructOpt)]
pub struct Info {
    /// Get info of this address (ss58address).
    address: String,
}

impl Info {
    /// execute command transfer
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let address = if self.address.starts_with("//") {
            Pair::from_string(&self.address, None)
                .expect("Parse development address failed")
                .public()
                .to_ss58check()
        } else {
            self.address.clone()
        };

        let info = signer.info(&address).await?;

        println!("{info:#?}");

        Ok(())
    }
}
