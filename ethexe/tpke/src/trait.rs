use crate::{
    TpkeError, aead,
    bls12_381::{hash_to_g1, serialize_g2},
    keys::{Encrypted, MasterPublicKey},
};
use ark_bls12_381::G2Affine;
use ark_ec::AffineRepr;
use ark_std::rand::{CryptoRng, RngCore};
use parity_scale_codec::{Decode, Encode};

pub trait Encryptable {
    type Id: AsRef<[u8; 32]>;
    type EncryptedFields: Encode + Decode;

    fn derive_id(&self) -> Self::Id;

    pub fn encrypt<R>(
        &self,
        pk: &MasterPublicKey,
        rng: &mut R,
    ) -> Result<Encrypted<Self>, TpkeError>
    where
        R: RngCore + CryptoRng,
    {
        // Identity public key would make z = e(Q_id, 0) = 1_GT, derivable by anyone
        // who sees the ciphertext — confidentiality fully bypassed. Reject.
        if pk.0.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }

        let q_id = hash_to_g1(id)?;
        // Reject the (negligibly likely) malformed id that hashes to the identity.
        if q_id.is_zero() {
            return Err(TpkeError::HashToCurve);
        }

        let u_scalar = Fr::rand(rng);
        let u_point = (G2Affine::generator() * u_scalar).into_affine();
        let u_bytes = serialize_g2(&u_point)?;

        // z = e(Q_id, AggPub)^u = e(Q_id, g₂)^(S·u)
        let z_base = Bls12_381::pairing(q_id, pk.0);
        let z = z_base * u_scalar;
        let body = aead::encrypt_body(&z, id, &u_bytes, plaintext)?;

        Ok(Encrypted {
            u: u_bytes,
            id: *id,
            body,
        })
    }
}
