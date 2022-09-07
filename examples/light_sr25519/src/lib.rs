#![no_std]

use schnorrkel::PublicKey;

// check sp-core/sr25519.rs for details
const SIGNING_CONTEXT: &[u8] = b"substrate";

pub enum Error {
    BadSignature,
    BadPublicKey,
    VerificationFailed,
}

pub fn verify<P: AsRef<[u8]>, M: AsRef<[u8]>>(
    signature: &[u8],
    message: M,
    pubkey: P,
) -> Result<(), Error> {
    let signature =
        schnorrkel::Signature::from_bytes(signature).map_err(|_| Error::BadSignature)?;

    let pub_key = PublicKey::from_bytes(pubkey.as_ref()).map_err(|_| Error::BadPublicKey)?;

    pub_key
        .verify_simple(SIGNING_CONTEXT, message.as_ref(), &signature)
        .map(|_| ())
        .map_err(|_| Error::VerificationFailed)
}
