//! command `transfer`
use crate::{api::signer::Signer, result::Result};
use structopt::StructOpt;
use subxt::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

/// Transfer value.
///
/// # Note
///
/// Gear node is currently using the default properties of substrate for
/// [the staging testnet][0], and the decimals of 1 UNIT is 12 by default.
///
/// [0]: https://github.com/gear-tech/gear/blob/c01d0390cdf1031cb4eba940d0199d787ea480e0/node/src/chain_spec.rs#L218
#[derive(Debug, StructOpt)]
pub struct Transfer {
    /// Transfer to (ss58address).
    destination: String,
    /// Balance to transfer.
    value: u128,
}

impl Transfer {
    /// Execute command transfer.
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let address = signer.signer.account_id();

        println!("From: {}", address.to_ss58check());
        println!("To: {}", self.destination);
        println!("Value: {}", self.value);

        signer
            .transfer(AccountId32::from_ss58check(&self.destination)?, self.value)
            .await?;

        Ok(())
    }
}
