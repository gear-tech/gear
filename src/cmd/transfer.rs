//! command transfer
use crate::{
    api::{generated::api::balances::calls::Transfer as TransferCall, Api},
    keystore, Result,
};
use structopt::StructOpt;
use subxt::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

/// Transfer value.
///
/// # Note
///
/// Gear node is currently using the default properties of substrate for
/// [the staging testnet][0], and the deciamls of 1 UNIT is 12 by default.
///
/// [0]: https://github.com/gear-tech/gear/blob/c01d0390cdf1031cb4eba940d0199d787ea480e0/node/src/chain_spec.rs#L218
#[derive(Debug, StructOpt)]
pub struct Transfer {
    /// Gear node rpc endpoint.
    #[structopt(short, long)]
    endpoint: Option<String>,
    /// Password of the signer account.
    #[structopt(short, long)]
    passwd: Option<String>,
    /// Transfer to (ss58address).
    destination: String,
    /// Balance will be transfered.
    value: u128,
}

impl Transfer {
    /// Execute command transfer.
    pub async fn exec(&self) -> Result<()> {
        let passwd = self.passwd.as_deref();
        let pair = keystore::cache(passwd)?;
        let address = pair.account_id();

        let api = Api::new(self.endpoint.as_ref().map(|s| s.as_ref()), passwd).await?;
        let balance = api.get_balance(&address.to_ss58check()).await?;

        println!("Address: {address:?}");
        println!(
            "Current balance: {balance:?} ~= {} UNIT",
            balance / 10u128.pow(12)
        );

        api.transfer(TransferCall {
            dest: AccountId32::from_ss58check(&self.destination)?.into(),
            value: self.value,
        })
        .await?;

        let balance = api.get_balance(&address.to_ss58check()).await?;
        println!(
            "Current balance: {balance:?} ~= {} UNIT",
            balance / 10u128.pow(12)
        );

        Ok(())
    }
}
