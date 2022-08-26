//! keystore

pub mod json;
pub mod key;
pub mod node;

use crate::{
    api::config::GearConfig,
    keystore::key::Key,
    result::{Error, Result},
    utils,
};
use lazy_static::lazy_static;
use std::{
    fs,
    path::{Path, PathBuf},
};
use subxt::{sp_core::sr25519, PairSigner};

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
pub fn generate(passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = Key::generate_with_phrase::<sr25519::Pair>(passwd)?;
    fs::write(&*KEYSTORE_PATH, pair.1)?;

    Ok(pair.0)
}

/// Login with suri.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn login(suri: &str, passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = Key::from_string(suri).pair::<sr25519::Pair>(passwd)?;
    fs::write(&*KEYSTORE_PATH, suri)?;

    Ok(pair.0)
}

/// Get signer from cache.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn cache(passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = if (*KEYSTORE_PATH).exists() {
        let suri = fs::read_to_string(&*KEYSTORE_PATH).map_err(|_| Error::Logout)?;
        Key::from_string(&suri).pair::<sr25519::Pair>(passwd)?.0
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
) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let encrypted = serde_json::from_slice::<json::Encrypted>(&fs::read(&path)?)?;
    let pair = encrypted.create(passphrase.ok_or(Error::InvalidPassword)?)?;

    fs::copy(path, &*KEYSTORE_JSON_PATH)?;
    Ok(PairSigner::new(pair))
}
