use crate::{Digest, PrivateKey, PublicKey};
use anyhow::{Context, Result};
use parity_scale_codec::{Decode, Encode};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message,
};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RawSignature([u8; 65]);

impl RawSignature {
    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<RawSignature> {
        let secp_secret_key = secp256k1::SecretKey::from_slice(&private_key.0)
            .with_context(|| "Invalid secret key format for {:?}")?;

        let message = Message::from_digest(digest.into());

        let recoverable =
            secp256k1::global::SECP256K1.sign_ecdsa_recoverable(&message, &secp_secret_key);

        let (id, signature) = recoverable.serialize_compact();
        let mut bytes = [0u8; 65];
        bytes[..64].copy_from_slice(signature.as_ref());
        bytes[64] = id.to_i32() as u8;
        Ok(RawSignature(bytes))
    }
}

impl From<RawSignature> for [u8; 65] {
    fn from(sig: RawSignature) -> [u8; 65] {
        sig.0
    }
}

impl AsRef<[u8]> for RawSignature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Signature> for RawSignature {
    fn from(mut sig: Signature) -> RawSignature {
        sig.0[64] -= 27;
        RawSignature(sig.0)
    }
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq)]
pub struct Signature([u8; 65]);

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn recover_from_digest(&self, digest: Digest) -> Result<PublicKey> {
        let sig = (*self).try_into()?;
        let public_key = secp256k1::global::SECP256K1
            .recover_ecdsa(&Message::from_digest(digest.into()), &sig)?;
        Ok(PublicKey::from_bytes(public_key.serialize()))
    }

    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<Signature> {
        let raw_signature = RawSignature::create_for_digest(private_key, digest)?;
        Ok(raw_signature.into())
    }
}

impl From<RawSignature> for Signature {
    fn from(mut sig: RawSignature) -> Self {
        sig.0[64] += 27;
        Signature(sig.0)
    }
}

impl From<Signature> for [u8; 65] {
    fn from(sig: Signature) -> [u8; 65] {
        sig.0
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Default for Signature {
    fn default() -> Self {
        Signature([0u8; 65])
    }
}

impl TryFrom<Signature> for RecoverableSignature {
    type Error = anyhow::Error;

    fn try_from(sig: Signature) -> Result<Self> {
        RecoverableSignature::from_compact(
            sig.0[..64].as_ref(),
            RecoveryId::from_i32((sig.0[64] - 27) as i32)?,
        )
        .map_err(Into::into)
    }
}
