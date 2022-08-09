//! keystore

mod json;

use crate::{api::config::GearConfig, utils, Error, Result};
use lazy_static::lazy_static;
use std::{
    fs,
    path::{Path, PathBuf},
};
use subxt::{
    sp_core::{sr25519::Pair, Pair as PairT},
    PairSigner,
};

lazy_static! {
    // @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    // when you have NO PASSWORD, If it can be got by an attacker then
    // they can also get your key.
    static ref KEYSTORE_PATH: PathBuf = utils::home().join("keystore");

    // @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    // when you have NO PASSWORD, If it can be got by an attacker then
    // they can also get your key.
    static ref KEYSTORE_JSON_PATH: PathBuf = utils::home().join("keystore.json");
}

/// Generate a new keypair.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn generate(passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let pair = Pair::generate_with_phrase(passwd);
    fs::write(&*KEYSTORE_PATH, pair.1)?;

    Ok(PairSigner::new(pair.0))
}

/// Login with suri.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn login(suri: &str, passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let pair = Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?;
    fs::write(&*KEYSTORE_PATH, suri)?;

    Ok(PairSigner::new(pair))
}

/// Get signer from cache.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn cache(passwd: Option<&str>) -> Result<PairSigner<GearConfig, Pair>> {
    let pair = if (*KEYSTORE_PATH).exists() {
        let suri = fs::read_to_string(&*KEYSTORE_PATH).map_err(|_| Error::Logout)?;
        let pair = Pair::from_string(&suri, passwd).map_err(|_| Error::InvalidSecret)?;
        PairSigner::new(pair)
    } else if (*KEYSTORE_JSON_PATH).exists() {
        decode_json_file(&*KEYSTORE_JSON_PATH, passwd)?
    } else {
        return Err(Error::Logout);
    };

    Ok(pair)
}

/// Decode pair from json file.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn decode_json_file(
    path: impl AsRef<Path>,
    passphrase: Option<&str>,
) -> Result<PairSigner<GearConfig, Pair>> {
    let encrypted = serde_json::from_slice::<json::Encrypted>(&fs::read(&path)?)?;
    let pair = encrypted.create(passphrase.ok_or(Error::InvalidPassword)?)?;

    fs::copy(path, &*KEYSTORE_JSON_PATH)?;
    Ok(PairSigner::new(pair))
}
