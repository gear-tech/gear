//! command login
use crate::{keystore, Result};
use structopt::StructOpt;

/// Login to account
#[derive(Debug, StructOpt)]
pub struct Login {
    /// The default keystore path is ~/.gear/keystore
    ///
    /// If no suri provided, will generate a new pair by default
    ///
    /// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    /// when you have NO PASSWORD, If it can be got by an attacker then
    /// they can also get your key.
    ///
    /// - If `s` is a possibly `0x` prefixed 64-digit hex string, then it will be interpreted
    /// directly as a `MiniSecretKey` (aka "seed" in `subkey`).
    ///
    /// - If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words, then the key will
    /// be derived from it. In this case:
    ///   - the phrase may be followed by one or more items delimited by `/` characters.
    ///   - the path may be followed by `///`, in which case everything after the `///` is treated
    /// as a password.
    ///
    /// - If `s` begins with a `/` character it is prefixed with the Substrate public `DEV_PHRASE`
    ///   and
    /// interpreted as above.
    pub suri: Option<String>,

    /// password of the signer account
    #[structopt(short, long)]
    pub passwd: Option<String>,
}

impl Login {
    /// exec command login
    pub fn exec(&self) -> Result<()> {
        let signer = if let Some(suri) = &self.suri {
            keystore::login(&suri, self.passwd.as_ref().map(|p| p.as_ref()))?
        } else {
            keystore::generate(self.passwd.as_ref().map(|p| p.as_ref()))?
        };

        println!("Successfully logged in as {}!", signer.account_id());
        Ok(())
    }
}
