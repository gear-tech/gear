//! keystore
use crate::{api::config::GearConfig, utils, Error, Result};
use lazy_static::lazy_static;
use std::{fs, path::PathBuf};
use subxt::{
    sp_core::{sr25519::Pair, Pair as PairT},
    PairSigner,
};

lazy_static! {
    // @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    // when you have NO PASSWORD, If it can be got by an attacker then
    // they can also get your key.
    static ref KEYSTORE_PATH: PathBuf = utils::home().join("keystore");
}

/// generate a new keypair
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn generate(passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let pair = Pair::generate_with_phrase(passwd);
    fs::write(&*KEYSTORE_PATH, pair.1)?;

    Ok(PairSigner::new(pair.0))
}

/// login with suri
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn login(suri: &str, passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let pair = Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?;
    fs::write(&*KEYSTORE_PATH, suri)?;
    Ok(PairSigner::new(pair))
}

/// get signer from cache
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn cache(passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let suri = fs::read_to_string(&*KEYSTORE_PATH).map_err(|_| Error::Logout)?;
    let pair = Pair::from_string(&suri, passwd).map_err(|_| Error::InvalidSecret)?;
    Ok(PairSigner::new(pair))
}
