use anyhow::Result;
use gring::Keystore;
use schnorrkel::Keypair;

const POLKADOT_JS_PAIR: &[u8] = include_bytes!("../res/pair.json");

#[test]
fn polkadot_js() -> Result<()> {
    let store = serde_json::from_slice::<Keystore>(POLKADOT_JS_PAIR)?;

    assert!(store.decrypt(None).is_err());
    assert!(store.decrypt(Some(b"42")).is_err());
    assert!(store.decrypt(Some(b"000000")).is_ok());
    Ok(())
}

#[test]
fn scrypt() -> Result<()> {
    let passphrase = b"42";
    let pair = Keypair::generate();
    let store = Keystore::encrypt_scrypt(pair.clone().into(), passphrase)?;

    assert_eq!(pair.secret, store.decrypt_scrypt(b"42")?.secret);
    Ok(())
}

#[test]
fn nopasswd() -> Result<()> {
    let pair = Keypair::generate();
    let store = Keystore::encrypt_none(pair.clone().into())?;

    assert_eq!(pair.secret, store.decrypt_none()?.secret);
    Ok(())
}
