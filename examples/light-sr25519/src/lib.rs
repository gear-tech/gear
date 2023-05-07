#![no_std]

use schnorrkel::{PublicKey, Signature};

// Check sp-core/sr25519.rs for details
const SIGNING_CONTEXT: &[u8] = b"substrate";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Error {
    BadSignature,
    BadPublicKey,
    VerificationFailed,
}

pub fn verify(
    signature: impl AsRef<[u8]>,
    message: impl AsRef<[u8]>,
    pub_key: impl AsRef<[u8]>,
) -> Result<(), Error> {
    let signature = Signature::from_bytes(signature.as_ref()).map_err(|_| Error::BadSignature)?;
    let pub_key = PublicKey::from_bytes(pub_key.as_ref()).map_err(|_| Error::BadPublicKey)?;

    pub_key
        .verify_simple(SIGNING_CONTEXT, message.as_ref(), &signature)
        .map(|_| ())
        .map_err(|_| Error::VerificationFailed)
}
