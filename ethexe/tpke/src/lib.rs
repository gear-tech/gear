// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Threshold public-key encryption for ethexe private transactions.
//!
//! Construction: Boneh-Franklin identity-based TPKE on BLS12-381, with the
//! master secret split into n Shamir shares (threshold t). Encryption is
//! identity-bound: every ciphertext carries an `id` and a decryption share
//! produced for `id` only decrypts that one ciphertext.
//!
//! See `~/.claude/plans/prancy-nibbling-pony.md` for the locked design.
//!
//! Pairing orientation (Type-3 on BLS12-381):
//!   - `Q_id ∈ G1` via hash-to-curve (DST below)
//!   - master pubkey, share pubkeys, ephemeral U  ∈ G2
//!   - decryption shares                          ∈ G1
//!   - e: G1 × G2 → GT
//!
//! IND-CCA via ChaCha20-Poly1305 (KEM/DEM with HKDF-SHA256 key/nonce derivation).
//! The DEM AAD binds (id, U_bytes, chain_id, key_epoch_id) into the MAC.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::{Bls12_381, Fr, G1Affine, G1Projective, G2Affine};
use ark_ec::{
    AffineRepr, CurveGroup,
    hashing::{HashToCurve, curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::{Pairing, PairingOutput},
};
use ark_ff::{Field, UniformRand, Zero, field_hashers::DefaultFieldHasher};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{CryptoRng, RngCore};
use blake2::{Blake2b, Digest, digest::consts::U32};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Hash-to-curve domain separation tag, version-locked. Changing this string
/// invalidates every in-flight ciphertext — do not modify post-launch.
pub const DST_G1: &[u8] = b"ETHEXE-TPKE-V1-BLS12381G1_XMD:SHA-256_SSWU_RO_";

/// HKDF info prefix for the KEM-derived DEM key/nonce.
pub const HKDF_DEM_INFO: &[u8] = b"ethexe-tpke-dem-v1";

/// Blake2b domain tag for `id` derivation.
pub const ID_DOMAIN: &[u8] = b"ethexe-tpke-v1";

/// 32-byte ChaCha20-Poly1305 key length.
const DEM_KEY_LEN: usize = 32;
/// 12-byte ChaCha20-Poly1305 nonce length.
const DEM_NONCE_LEN: usize = 12;
/// Compressed G2 point byte length on BLS12-381.
pub const G2_COMPRESSED_LEN: usize = 96;
/// Compressed G1 point byte length on BLS12-381.
pub const G1_COMPRESSED_LEN: usize = 48;

type Blake2b256 = Blake2b<U32>;
type G1Hasher = MapToCurveBasedHasher<
    G1Projective,
    DefaultFieldHasher<Sha256, 128>,
    WBMap<ark_bls12_381::g1::Config>,
>;

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum TpkeError {
    #[error("malformed ciphertext envelope")]
    MalformedCiphertext,
    #[error("AEAD authentication failed")]
    AeadAuth,
    #[error("decryption share did not verify against share public key")]
    ShareVerification,
    #[error("not enough shares to combine: got {got}, need {need}")]
    InsufficientShares { got: usize, need: usize },
    #[error("duplicate share index {0}")]
    DuplicateShareIndex(u32),
    #[error("share index {0} is zero (validator ids start at 1)")]
    ZeroShareIndex(u32),
    #[error("share #{index} bound to a different envelope id than the target")]
    ShareEnvelopeMismatch { index: u32 },
    #[error("point serialization failed")]
    Serialization,
    #[error("hash-to-curve failed")]
    HashToCurve,
    #[error("invalid threshold: t={t}, n={n} (require 1 <= t <= n)")]
    InvalidThreshold { t: u32, n: u32 },
    #[error("public key is the identity point — refusing to use it")]
    IdentityPublicKey,
}

/// Master secret key produced by the dealer. Must be destroyed after splitting.
///
/// The scalar field is private — callers can construct via [`Self::new`] and
/// read it via [`Self::scalar`], but cannot accidentally print or copy it
/// through direct field access. `Debug` is implemented to elide the scalar.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterSecretKey(Fr);

impl MasterSecretKey {
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }
    pub fn scalar(&self) -> Fr {
        self.0
    }
}

impl core::fmt::Debug for MasterSecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never leak the scalar via Debug.
        f.write_str("MasterSecretKey(<redacted>)")
    }
}

/// Per-validator secret share `Sᵢ = f(i)`. Index is 1-based.
///
/// `scalar` is private; use [`Self::new`] to construct and [`Self::scalar`]
/// to read. The `index` is non-sensitive and stays public.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKeyShare {
    pub index: u32,
    scalar: Fr,
}

impl SecretKeyShare {
    pub fn new(index: u32, scalar: Fr) -> Self {
        Self { index, scalar }
    }
    pub fn scalar(&self) -> Fr {
        self.scalar
    }
}

impl core::fmt::Debug for SecretKeyShare {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SecretKeyShare")
            .field("index", &self.index)
            .field("scalar", &"<redacted>")
            .finish()
    }
}

/// Master public key `AggPub = S · g₂ ∈ G2`. Published openly.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MasterPublicKey(pub G2Affine);

/// Per-validator share public key `PSᵢ = Sᵢ · g₂ ∈ G2`. Used by anyone to
/// verify a decryption share without knowing the secret.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SharePublicKey {
    pub index: u32,
    pub point: G2Affine,
}

/// Encrypted ciphertext envelope. Wire format is the SCALE encoding of this.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct EncryptedEnvelope {
    /// `U = u · g₂ ∈ G2`, compressed 96-byte serialization.
    pub u: [u8; G2_COMPRESSED_LEN],
    /// 32-byte identity binding (see `derive_id`).
    pub id: [u8; 32],
    /// ChaCha20-Poly1305 ciphertext incl. 16-byte Poly1305 tag.
    pub body: Vec<u8>,
}

/// Decryption share `Dᵢ = Sᵢ · Q_id ∈ G1`. Validator index is 1-based.
///
/// The `id` field binds the share to the envelope it was produced for. `verify`
/// and `combine` reject shares whose id doesn't match the target envelope —
/// this prevents accidental cross-envelope mixing from silently producing
/// garbage plaintext (which would otherwise only surface as a cryptic AEAD
/// failure downstream).
///
/// SCALE wire format: `index: u32` ‖ `id: [u8; 32]` ‖ `compressed_point: [u8; 48]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DecryptionShare {
    pub index: u32,
    pub id: [u8; 32],
    pub point: G1Affine,
}

// SCALE codec for wire types. Manual impls are needed because arkworks' point
// types don't implement `Encode`/`Decode`/`TypeInfo`. The wire format uses
// BLS12-381 compressed encodings (48 B for G1, 96 B for G2). Encode panics
// only on serialization failure of an in-memory valid point, which cannot
// happen for points produced by this crate; Decode validates and returns a
// codec error on bad bytes.

impl Encode for DecryptionShare {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        self.index.encode_to(dest);
        self.id.encode_to(dest);
        let bytes =
            serialize_g1(&self.point).expect("DecryptionShare always holds a valid G1 point");
        bytes.encode_to(dest);
    }
}

impl Decode for DecryptionShare {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let index = u32::decode(input)?;
        let id = <[u8; 32]>::decode(input)?;
        let bytes = <[u8; G1_COMPRESSED_LEN]>::decode(input)?;
        let point = deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(&bytes)
            .map_err(|_| parity_scale_codec::Error::from("invalid G1 point in DecryptionShare"))?;
        Ok(Self { index, id, point })
    }
}

impl Encode for MasterPublicKey {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        let bytes = serialize_g2(&self.0).expect("MasterPublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for MasterPublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let bytes = <[u8; G2_COMPRESSED_LEN]>::decode(input)?;
        // Use from_bytes so identity-point rejection is centralized.
        Self::from_bytes(&bytes).map_err(|_| {
            parity_scale_codec::Error::from("invalid or identity G2 point in MasterPublicKey")
        })
    }
}

impl Encode for SharePublicKey {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        self.index.encode_to(dest);
        let bytes =
            serialize_g2(&self.point).expect("SharePublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for SharePublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let index = u32::decode(input)?;
        let bytes = <[u8; G2_COMPRESSED_LEN]>::decode(input)?;
        Self::from_bytes(index, &bytes).map_err(|_| {
            parity_scale_codec::Error::from("invalid or identity G2 point in SharePublicKey")
        })
    }
}

// ---------------------------------------------------------------------------
// Identity binding
// ---------------------------------------------------------------------------

/// Derive the 32-byte identity that binds a ciphertext to its plaintext,
/// chain, and key epoch.
///
/// `user_nonce` MUST be high-entropy randomness chosen at encryption time.
/// Without it, an attacker who can guess plaintext (e.g. known token-trade
/// templates) can verify the guess by recomputing `id` and matching the
/// ciphertext's id — a known-plaintext attack on the identity.
pub fn derive_id(
    chain_id: u64,
    key_epoch_id: u32,
    canonical_plaintext: &[u8],
    user_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut h = Blake2b256::new();
    h.update(ID_DOMAIN);
    h.update(chain_id.to_le_bytes());
    h.update(key_epoch_id.to_le_bytes());
    h.update(canonical_plaintext);
    h.update(user_nonce);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    out
}

// ---------------------------------------------------------------------------
// Dealer (Shamir secret sharing)
// ---------------------------------------------------------------------------

impl MasterSecretKey {
    /// Run the dealer ceremony locally: sample a fresh master secret, split
    /// it into `n` Shamir shares with threshold `t`, and return shares + pubs.
    ///
    /// `t` is the number of shares required to decrypt. `n` is the total
    /// validator count. Indices in the returned shares are 1..=n.
    ///
    /// The returned `MasterSecretKey` SHOULD be zeroized by the caller as soon
    /// as the shares are persisted off-machine (`drop` does this on Drop).
    pub fn deal<R: RngCore + CryptoRng>(
        t: u32,
        n: u32,
        rng: &mut R,
    ) -> Result<DealerOutput, TpkeError> {
        if t == 0 || n == 0 || t > n {
            return Err(TpkeError::InvalidThreshold { t, n });
        }
        // Sample polynomial coefficients: f(x) = a_0 + a_1·x + ... + a_{t-1}·x^{t-1}
        // where a_0 = S (master secret).
        let mut coeffs: Vec<Fr> = (0..t).map(|_| Fr::rand(rng)).collect();
        let master = MasterSecretKey::new(coeffs[0]);

        // Compute share Sᵢ = f(i) for i in 1..=n using Horner's rule.
        let mut shares = Vec::with_capacity(n as usize);
        let mut share_pubs = Vec::with_capacity(n as usize);
        let g2 = G2Affine::generator();
        for i in 1..=n {
            let x = Fr::from(i as u64);
            // Horner: acc = a_{t-1}; for k in (t-2..=0): acc = acc·x + a_k.
            let mut acc = coeffs[t as usize - 1];
            for k in (0..t as usize - 1).rev() {
                acc = acc * x + coeffs[k];
            }
            let sk = SecretKeyShare::new(i, acc);
            let pk = SharePublicKey {
                index: i,
                point: (g2 * acc).into_affine(),
            };
            shares.push(sk);
            share_pubs.push(pk);
        }

        let master_pub = MasterPublicKey((g2 * master.scalar()).into_affine());

        // Wipe intermediate polynomial coefficients.
        coeffs.zeroize();

        Ok(DealerOutput {
            master_secret: Some(master),
            master_pub,
            shares,
            share_pubs,
        })
    }
}

/// Output of the dealer ceremony.
///
/// The `master_secret` is held in an `Option` and is accessible only via
/// [`take_master_secret`]. This makes the destruction step explicit: take it
/// once to persist or hand off, then let it drop (zeroized on drop). Cloning
/// `DealerOutput` clones the shares + pubs but does NOT clone the master
/// secret — subsequent clones see `None`. A leftover `master_secret` inside
/// `DealerOutput` is zeroized when the struct is dropped.
///
/// [`take_master_secret`]: DealerOutput::take_master_secret
#[derive(Debug)]
pub struct DealerOutput {
    master_secret: Option<MasterSecretKey>,
    pub master_pub: MasterPublicKey,
    pub shares: Vec<SecretKeyShare>,
    pub share_pubs: Vec<SharePublicKey>,
}

impl DealerOutput {
    /// Take ownership of the master secret. Returns `None` if it has already
    /// been taken or never existed. Subsequent calls return `None`.
    pub fn take_master_secret(&mut self) -> Option<MasterSecretKey> {
        self.master_secret.take()
    }
}

impl Clone for DealerOutput {
    fn clone(&self) -> Self {
        // Deliberately drop the master secret on clone — see struct docs.
        Self {
            master_secret: None,
            master_pub: self.master_pub.clone(),
            shares: self.shares.clone(),
            share_pubs: self.share_pubs.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Hash to G1
// ---------------------------------------------------------------------------

// The hasher is constructed once per process and reused. DST_G1 is constant,
// so `G1Hasher::new` only fails for malformed DSTs — which we control at
// compile time — making the `.expect()` unreachable in practice.
#[cfg(feature = "std")]
fn g1_hasher() -> &'static G1Hasher {
    use std::sync::OnceLock;
    static HASHER: OnceLock<G1Hasher> = OnceLock::new();
    HASHER.get_or_init(|| G1Hasher::new(DST_G1).expect("DST_G1 is a valid hash-to-curve DST"))
}

fn hash_to_g1(id: &[u8; 32]) -> Result<G1Affine, TpkeError> {
    #[cfg(feature = "std")]
    {
        g1_hasher().hash(id).map_err(|_| TpkeError::HashToCurve)
    }
    #[cfg(not(feature = "std"))]
    {
        let hasher = G1Hasher::new(DST_G1).map_err(|_| TpkeError::HashToCurve)?;
        hasher.hash(id).map_err(|_| TpkeError::HashToCurve)
    }
}

// ---------------------------------------------------------------------------
// Encrypt
// ---------------------------------------------------------------------------

/// Additional Authenticated Data layout (input to ChaCha20-Poly1305 MAC).
///
/// Format: id ‖ U_bytes ‖ chain_id_le ‖ key_epoch_id_le
fn build_aad(envelope_id: &[u8; 32], u_bytes: &[u8], chain_id: u64, key_epoch_id: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + G2_COMPRESSED_LEN + 8 + 4);
    aad.extend_from_slice(envelope_id);
    aad.extend_from_slice(u_bytes);
    aad.extend_from_slice(&chain_id.to_le_bytes());
    aad.extend_from_slice(&key_epoch_id.to_le_bytes());
    aad
}

/// Serialize an arkworks point/element to its fixed-size compressed bytes.
fn serialize_compressed<P: CanonicalSerialize, const N: usize>(
    p: &P,
) -> Result<[u8; N], TpkeError> {
    let mut buf = [0u8; N];
    p.serialize_compressed(&mut buf[..])
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Deserialize a fixed-size compressed-bytes blob into an arkworks point.
fn deserialize_compressed<P: CanonicalDeserialize, const N: usize>(
    bytes: &[u8; N],
) -> Result<P, TpkeError> {
    P::deserialize_compressed(&bytes[..]).map_err(|_| TpkeError::MalformedCiphertext)
}

fn serialize_g2(p: &G2Affine) -> Result<[u8; G2_COMPRESSED_LEN], TpkeError> {
    serialize_compressed::<_, G2_COMPRESSED_LEN>(p)
}

fn deserialize_g2(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<G2Affine, TpkeError> {
    deserialize_compressed::<G2Affine, G2_COMPRESSED_LEN>(bytes)
}

fn serialize_g1(p: &G1Affine) -> Result<[u8; G1_COMPRESSED_LEN], TpkeError> {
    serialize_compressed::<_, G1_COMPRESSED_LEN>(p)
}

/// Encode the pairing-target element `z ∈ GT` deterministically for HKDF input.
fn serialize_gt(z: &PairingOutput<Bls12_381>) -> Result<Vec<u8>, TpkeError> {
    let mut buf = Vec::with_capacity(576);
    z.serialize_compressed(&mut buf)
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Derive the 44 raw bytes (32-byte AEAD key + 12-byte AEAD nonce) from the
/// shared secret `z`, identity, and ephemeral `U`.
fn derive_dem_key_nonce(
    z: &PairingOutput<Bls12_381>,
    envelope_id: &[u8; 32],
    u_bytes: &[u8],
) -> Result<([u8; DEM_KEY_LEN], [u8; DEM_NONCE_LEN]), TpkeError> {
    let z_bytes = serialize_gt(z)?;
    let mut info = Vec::with_capacity(HKDF_DEM_INFO.len() + 32 + u_bytes.len());
    info.extend_from_slice(HKDF_DEM_INFO);
    info.extend_from_slice(envelope_id);
    info.extend_from_slice(u_bytes);

    let hk = Hkdf::<Sha256>::new(None, &z_bytes);
    let mut okm = [0u8; DEM_KEY_LEN + DEM_NONCE_LEN];
    hk.expand(&info, &mut okm)
        .map_err(|_| TpkeError::Serialization)?;

    let mut key = [0u8; DEM_KEY_LEN];
    let mut nonce = [0u8; DEM_NONCE_LEN];
    key.copy_from_slice(&okm[..DEM_KEY_LEN]);
    nonce.copy_from_slice(&okm[DEM_KEY_LEN..]);
    Ok((key, nonce))
}

/// Encrypt `plaintext` for identity `id` under master public key `pk`.
///
/// `chain_id` and `key_epoch_id` are bound into the AEAD's AAD so a ciphertext
/// can only be decrypted within its intended chain+epoch context.
pub fn encrypt<R: RngCore + CryptoRng>(
    pk: &MasterPublicKey,
    id: &[u8; 32],
    chain_id: u64,
    key_epoch_id: u32,
    plaintext: &[u8],
    rng: &mut R,
) -> Result<EncryptedEnvelope, TpkeError> {
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

    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(&z, id, &u_bytes)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(id, &u_bytes, chain_id, key_epoch_id);
    let body = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| TpkeError::AeadAuth)?;

    Ok(EncryptedEnvelope {
        u: u_bytes,
        id: *id,
        body,
    })
}

// ---------------------------------------------------------------------------
// Decryption share emit + verify
// ---------------------------------------------------------------------------

impl SecretKeyShare {
    /// Validator-side: produce `Dᵢ = Sᵢ · Q_id` for the ciphertext's id.
    pub fn decrypt_share(
        &self,
        envelope: &EncryptedEnvelope,
    ) -> Result<DecryptionShare, TpkeError> {
        if self.index == 0 {
            return Err(TpkeError::ZeroShareIndex(0));
        }
        let q_id = hash_to_g1(&envelope.id)?;
        let point = (q_id * self.scalar).into_affine();
        Ok(DecryptionShare {
            index: self.index,
            id: envelope.id,
            point,
        })
    }
}

impl SharePublicKey {
    /// Verify a decryption share: e(Dᵢ, g₂) ?= e(Q_id, PSᵢ).
    ///
    /// Returns `Ok(false)` when the share's validator index or envelope id
    /// doesn't match what we're verifying against.
    pub fn verify(
        &self,
        envelope: &EncryptedEnvelope,
        share: &DecryptionShare,
    ) -> Result<bool, TpkeError> {
        if share.index != self.index || share.id != envelope.id {
            return Ok(false);
        }
        let q_id = hash_to_g1(&envelope.id)?;
        let g2 = G2Affine::generator();
        let lhs = Bls12_381::pairing(share.point, g2);
        let rhs = Bls12_381::pairing(q_id, self.point);
        Ok(lhs == rhs)
    }
}

// ---------------------------------------------------------------------------
// Combine + decrypt
// ---------------------------------------------------------------------------

/// Compute Lagrange coefficient `λᵢ = ∏_{j != i} (j / (j - i))` evaluated at 0.
///
/// Indices are 1-based validator ids. Returns `None` if any (j - i) is zero
/// (caller must dedupe before calling).
fn lagrange_coefficient(i: u32, indices: &[u32]) -> Option<Fr> {
    let xi = Fr::from(i as u64);
    let mut num = Fr::from(1u64);
    let mut den = Fr::from(1u64);
    for &j in indices {
        if j == i {
            continue;
        }
        let xj = Fr::from(j as u64);
        num *= xj; // numerator term: x_j (since we evaluate at x = 0: 0 - x_j = -x_j; signs cancel)
        let diff = xj - xi;
        if diff.is_zero() {
            return None;
        }
        den *= diff;
    }
    let den_inv = den.inverse()?;
    Some(num * den_inv)
}

/// Combine `t` decryption shares into the plaintext.
///
/// This function does NOT verify shares cryptographically — callers must run
/// `SharePublicKey::verify` on each share they trust as honest. It DOES enforce:
///   - share count ≥ threshold
///   - every share's `id` matches `envelope.id`
///   - no zero or duplicate validator indices
///
/// **Only the first `threshold` shares from `shares` are consumed.** Excess
/// shares are ignored. To use a specific subset, slice the input yourself.
pub fn combine(
    envelope: &EncryptedEnvelope,
    shares: &[DecryptionShare],
    chain_id: u64,
    key_epoch_id: u32,
    threshold: u32,
) -> Result<Vec<u8>, TpkeError> {
    // Match deal()'s invariant: threshold 0 would short-circuit to the G1
    // identity for D and let anyone "decrypt" ciphertexts produced under an
    // identity master pubkey. Reject up front.
    if threshold == 0 {
        return Err(TpkeError::InvalidThreshold { t: 0, n: 0 });
    }
    if (shares.len() as u32) < threshold {
        return Err(TpkeError::InsufficientShares {
            got: shares.len(),
            need: threshold as usize,
        });
    }
    // Use the first `threshold` shares.
    let used = &shares[..threshold as usize];

    // Reject zero/duplicate indices and envelope mismatches.
    let mut seen: Vec<u32> = Vec::with_capacity(used.len());
    for s in used {
        if s.index == 0 {
            return Err(TpkeError::ZeroShareIndex(0));
        }
        if s.id != envelope.id {
            return Err(TpkeError::ShareEnvelopeMismatch { index: s.index });
        }
        if seen.contains(&s.index) {
            return Err(TpkeError::DuplicateShareIndex(s.index));
        }
        seen.push(s.index);
    }

    // D = Σ λᵢ · Dᵢ (Lagrange in the exponent)
    let mut acc = G1Projective::zero();
    for s in used {
        let lambda =
            lagrange_coefficient(s.index, &seen).ok_or(TpkeError::DuplicateShareIndex(s.index))?;
        acc += s.point * lambda;
    }
    let d = acc.into_affine();

    // z' = e(D, U)
    let u_point = deserialize_g2(&envelope.u)?;
    let z = Bls12_381::pairing(d, u_point);

    // Derive DEM key+nonce identically to encrypt, decrypt body.
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(&z, &envelope.id, &envelope.u)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(&envelope.id, &envelope.u, chain_id, key_epoch_id);
    cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &envelope.body,
                aad: &aad,
            },
        )
        .map_err(|_| TpkeError::AeadAuth)
}

// ---------------------------------------------------------------------------
// Serialization helpers for testing / wire interop
// ---------------------------------------------------------------------------

impl DecryptionShare {
    /// Serialize as `(index, id, compressed_point_bytes)`.
    pub fn to_bytes(&self) -> Result<(u32, [u8; 32], [u8; G1_COMPRESSED_LEN]), TpkeError> {
        Ok((self.index, self.id, serialize_g1(&self.point)?))
    }

    pub fn from_bytes(
        index: u32,
        id: [u8; 32],
        bytes: &[u8; G1_COMPRESSED_LEN],
    ) -> Result<Self, TpkeError> {
        let point = deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(bytes)?;
        Ok(Self { index, id, point })
    }
}

impl MasterPublicKey {
    pub fn to_bytes(&self) -> Result<[u8; G2_COMPRESSED_LEN], TpkeError> {
        serialize_g2(&self.0)
    }

    /// Deserialize the master pubkey, rejecting the G2 identity point. An
    /// identity-element master pubkey would make `e(Q_id, pk) = 1_GT`, letting
    /// anyone with the ciphertext derive the DEM key without shares.
    pub fn from_bytes(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self, TpkeError> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self(point))
    }
}

impl SharePublicKey {
    pub fn to_bytes(&self) -> Result<(u32, [u8; G2_COMPRESSED_LEN]), TpkeError> {
        Ok((self.index, serialize_g2(&self.point)?))
    }

    /// Deserialize a share pubkey, rejecting the G2 identity point. An identity
    /// share-pubkey would make share verification accept any honest share for
    /// that index regardless of the underlying secret-share scalar.
    pub fn from_bytes(index: u32, bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self, TpkeError> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self { index, point })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::{SeedableRng, rngs::StdRng};

    fn fixed_rng() -> StdRng {
        StdRng::seed_from_u64(0xDEADBEEF_CAFEBABE)
    }

    fn deal(t: u32, n: u32) -> DealerOutput {
        let mut rng = fixed_rng();
        MasterSecretKey::deal(t, n, &mut rng).unwrap()
    }

    #[test]
    fn roundtrip_4_of_7() {
        let d = deal(4, 7);
        let mut rng = fixed_rng();
        let id = derive_id(42, 1, b"hello world", &[7u8; 32]);
        let env = encrypt(&d.master_pub, &id, 42, 1, b"hello world", &mut rng).unwrap();
        let shares: Vec<DecryptionShare> = d
            .shares
            .iter()
            .take(4)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        // Verify each share.
        for (s, ps) in shares.iter().zip(d.share_pubs.iter()) {
            assert!(ps.verify(&env, s).unwrap());
        }
        let pt = combine(&env, &shares, 42, 1, 4).unwrap();
        assert_eq!(pt, b"hello world");
    }

    #[test]
    fn insufficient_shares_returns_err() {
        let d = deal(3, 5);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"x", &[0u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
        let shares: Vec<_> = d
            .shares
            .iter()
            .take(2)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        let err = combine(&env, &shares, 1, 0, 3).unwrap_err();
        assert!(matches!(
            err,
            TpkeError::InsufficientShares { got: 2, need: 3 }
        ));
    }

    #[test]
    fn mutated_ciphertext_fails_aead() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[1u8; 32]);
        let mut env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        // Flip one bit of the ciphertext body.
        env.body[0] ^= 1;
        let shares: Vec<_> = d
            .shares
            .iter()
            .take(2)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        let err = combine(&env, &shares, 1, 0, 2).unwrap_err();
        assert!(matches!(err, TpkeError::AeadAuth));
    }

    #[test]
    fn wrong_aad_chain_id_fails_aead() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[1u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        let shares: Vec<_> = d
            .shares
            .iter()
            .take(2)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        // Decrypt with wrong chain_id.
        let err = combine(&env, &shares, 999, 0, 2).unwrap_err();
        assert!(matches!(err, TpkeError::AeadAuth));
    }

    #[test]
    fn share_for_wrong_validator_fails_verify() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[1u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        // Validator 1 produces a share but we check it against validator 2's pubkey.
        let share1 = d.shares[0].decrypt_share(&env).unwrap();
        // Swap index to 2 to bypass the index-mismatch shortcut and trigger the pairing check.
        let mut fake = share1.clone();
        fake.index = 2;
        assert!(!d.share_pubs[1].verify(&env, &fake).unwrap());
    }

    #[test]
    fn mutated_id_breaks_share_verification() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[1u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        let share = d.shares[0].decrypt_share(&env).unwrap();
        let mut tampered = env.clone();
        tampered.id[0] ^= 0xFF;
        // Share was for original id, not the tampered one: pairing check fails.
        assert!(!d.share_pubs[0].verify(&tampered, &share).unwrap());
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"", &[2u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"", &mut rng).unwrap();
        let shares: Vec<_> = d
            .shares
            .iter()
            .take(2)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        let pt = combine(&env, &shares, 1, 0, 2).unwrap();
        assert_eq!(pt, b"");
    }

    #[test]
    fn duplicate_share_index_rejected() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"x", &[3u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
        let share = d.shares[0].decrypt_share(&env).unwrap();
        // Same share twice.
        let err = combine(&env, &[share.clone(), share.clone()], 1, 0, 2).unwrap_err();
        assert!(matches!(err, TpkeError::DuplicateShareIndex(1)));
    }

    #[test]
    fn invalid_threshold_at_deal_time() {
        let mut rng = fixed_rng();
        assert!(matches!(
            MasterSecretKey::deal(0, 3, &mut rng).unwrap_err(),
            TpkeError::InvalidThreshold { t: 0, n: 3 }
        ));
        assert!(matches!(
            MasterSecretKey::deal(5, 3, &mut rng).unwrap_err(),
            TpkeError::InvalidThreshold { t: 5, n: 3 }
        ));
    }

    #[test]
    fn id_derivation_is_deterministic_and_nonce_sensitive() {
        let a = derive_id(1, 0, b"hello", &[0u8; 32]);
        let b = derive_id(1, 0, b"hello", &[0u8; 32]);
        let c = derive_id(1, 0, b"hello", &[1u8; 32]);
        let d = derive_id(2, 0, b"hello", &[0u8; 32]);
        let e = derive_id(1, 1, b"hello", &[0u8; 32]);
        assert_eq!(a, b);
        assert_ne!(a, c, "different nonce must give different id");
        assert_ne!(a, d, "different chain_id must give different id");
        assert_ne!(a, e, "different key_epoch_id must give different id");
    }

    #[test]
    fn envelope_scale_roundtrip() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[4u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        let encoded = env.encode();
        let decoded = EncryptedEnvelope::decode(&mut &encoded[..]).unwrap();
        assert_eq!(env, decoded);
    }

    #[test]
    fn decryption_share_scale_roundtrip() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[5u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        let share = d.shares[0].decrypt_share(&env).unwrap();
        let encoded = share.encode();
        // 4 (index) + 32 (id) + 48 (G1 compressed) = 84 bytes.
        assert_eq!(encoded.len(), 4 + 32 + 48);
        let decoded = DecryptionShare::decode(&mut &encoded[..]).unwrap();
        assert_eq!(share, decoded);
    }

    #[test]
    fn master_pub_scale_roundtrip() {
        let d = deal(2, 3);
        let encoded = d.master_pub.encode();
        assert_eq!(encoded.len(), G2_COMPRESSED_LEN);
        let decoded = MasterPublicKey::decode(&mut &encoded[..]).unwrap();
        assert_eq!(d.master_pub, decoded);
    }

    #[test]
    fn share_pub_scale_roundtrip() {
        let d = deal(2, 3);
        let encoded = d.share_pubs[0].encode();
        assert_eq!(encoded.len(), 4 + G2_COMPRESSED_LEN);
        let decoded = SharePublicKey::decode(&mut &encoded[..]).unwrap();
        assert_eq!(d.share_pubs[0], decoded);
    }

    #[test]
    fn scale_decode_rejects_identity_master_pub() {
        // Serialize the G2 identity and try to SCALE-decode as a MasterPublicKey.
        let mut buf = [0u8; G2_COMPRESSED_LEN];
        G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
        let encoded = buf.encode();
        assert!(MasterPublicKey::decode(&mut &encoded[..]).is_err());
    }

    #[test]
    fn arbitrary_subset_of_t_shares_works() {
        // 3-of-5: any 3 distinct shares must combine to the same plaintext.
        let d = deal(3, 5);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"abc", &[5u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
        let all: Vec<_> = d
            .shares
            .iter()
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect();
        for subset_indices in [[0, 1, 2], [0, 1, 4], [1, 3, 4], [2, 3, 4]] {
            let subset: Vec<_> = subset_indices.iter().map(|&i| all[i].clone()).collect();
            let pt = combine(&env, &subset, 1, 0, 3).unwrap();
            assert_eq!(pt, b"abc", "subset {subset_indices:?} failed");
        }
    }

    #[test]
    fn take_master_secret_is_one_shot() {
        let mut d = deal(2, 3);
        assert!(d.take_master_secret().is_some());
        assert!(d.take_master_secret().is_none());
        // Subsequent state is otherwise intact.
        assert_eq!(d.shares.len(), 3);
    }

    #[test]
    fn clone_dealer_output_drops_master_secret() {
        let d = deal(2, 3);
        let mut cloned = d.clone();
        // Clone never carries the secret.
        assert!(cloned.take_master_secret().is_none());
        // Pubs/shares still cloned.
        assert_eq!(cloned.shares.len(), 3);
        assert_eq!(cloned.share_pubs.len(), 3);
    }

    // Regression tests for codex review findings (PR gear-tech/gear#5427).
    //
    // [P1] An identity G2 master pubkey makes `e(Q_id, pk) = 1_GT`, letting
    // anyone derive the DEM key from the public envelope alone. Encryption
    // and pubkey deserialization must reject it.
    //
    // [P2] `combine(.., threshold=0)` would slice to an empty share set and
    // interpolate to the G1 identity, producing a usable D under an attacker-
    // controlled identity master pubkey. Reject it to mirror deal()'s rule.

    #[test]
    fn identity_master_pubkey_rejected_at_encrypt() {
        let identity_pk = MasterPublicKey(G2Affine::zero());
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"x", &[0u8; 32]);
        let err = encrypt(&identity_pk, &id, 1, 0, b"x", &mut rng).unwrap_err();
        assert!(matches!(err, TpkeError::IdentityPublicKey));
    }

    #[test]
    fn identity_master_pubkey_rejected_at_from_bytes() {
        let mut buf = [0u8; G2_COMPRESSED_LEN];
        G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
        let err = MasterPublicKey::from_bytes(&buf).unwrap_err();
        assert!(matches!(err, TpkeError::IdentityPublicKey));
    }

    #[test]
    fn identity_share_pubkey_rejected_at_from_bytes() {
        let mut buf = [0u8; G2_COMPRESSED_LEN];
        G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
        let err = SharePublicKey::from_bytes(1, &buf).unwrap_err();
        assert!(matches!(err, TpkeError::IdentityPublicKey));
    }

    #[test]
    fn zero_threshold_rejected_in_combine() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id = derive_id(1, 0, b"x", &[0u8; 32]);
        let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
        let err = combine(&env, &[], 1, 0, 0).unwrap_err();
        assert!(matches!(err, TpkeError::InvalidThreshold { t: 0, n: 0 }));
    }

    #[test]
    fn share_from_other_envelope_rejected_in_combine() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id_a = derive_id(1, 0, b"alpha", &[0xAAu8; 32]);
        let id_b = derive_id(1, 0, b"beta", &[0xBBu8; 32]);
        let env_a = encrypt(&d.master_pub, &id_a, 1, 0, b"alpha", &mut rng).unwrap();
        let env_b = encrypt(&d.master_pub, &id_b, 1, 0, b"beta", &mut rng).unwrap();
        // Take share #1 from envelope A and share #2 from envelope B.
        let s_a = d.shares[0].decrypt_share(&env_a).unwrap();
        let s_b = d.shares[1].decrypt_share(&env_b).unwrap();
        // Try to combine for envelope A — share #2 has the wrong id.
        let err = combine(&env_a, &[s_a, s_b], 1, 0, 2).unwrap_err();
        assert!(matches!(err, TpkeError::ShareEnvelopeMismatch { index: 2 }));
    }

    #[test]
    fn verify_rejects_wrong_envelope_id() {
        let d = deal(2, 3);
        let mut rng = fixed_rng();
        let id_a = derive_id(1, 0, b"alpha", &[0xAAu8; 32]);
        let id_b = derive_id(1, 0, b"beta", &[0xBBu8; 32]);
        let env_a = encrypt(&d.master_pub, &id_a, 1, 0, b"alpha", &mut rng).unwrap();
        let env_b = encrypt(&d.master_pub, &id_b, 1, 0, b"beta", &mut rng).unwrap();
        // Share is for env_a but we verify against env_b — must return false.
        let share = d.shares[0].decrypt_share(&env_a).unwrap();
        assert!(!d.share_pubs[0].verify(&env_b, &share).unwrap());
    }

    // ----- to_bytes / from_bytes roundtrip property tests -----

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 16, .. ProptestConfig::default() })]

        #[test]
        fn proptest_decryption_share_roundtrip(plaintext in proptest::collection::vec(any::<u8>(), 0..200)) {
            let d = deal(2, 3);
            let mut rng = fixed_rng();
            let id = derive_id(1, 0, &plaintext, &[7u8; 32]);
            let env = encrypt(&d.master_pub, &id, 1, 0, &plaintext, &mut rng).unwrap();
            let share = d.shares[0].decrypt_share(&env).unwrap();
            let (idx, id_bytes, point_bytes) = share.to_bytes().unwrap();
            let restored = DecryptionShare::from_bytes(idx, id_bytes, &point_bytes).unwrap();
            prop_assert_eq!(share, restored);
        }

        #[test]
        fn proptest_master_public_key_roundtrip(seed in any::<u64>()) {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut d = MasterSecretKey::deal(2, 3, &mut rng).unwrap();
            let _ = d.take_master_secret();
            let bytes = d.master_pub.to_bytes().unwrap();
            let restored = MasterPublicKey::from_bytes(&bytes).unwrap();
            prop_assert_eq!(d.master_pub, restored);
        }

        #[test]
        fn proptest_share_public_key_roundtrip(seed in any::<u64>()) {
            let mut rng = StdRng::seed_from_u64(seed);
            let d = MasterSecretKey::deal(3, 5, &mut rng).unwrap();
            for ps in &d.share_pubs {
                let (idx, bytes) = ps.to_bytes().unwrap();
                let restored = SharePublicKey::from_bytes(idx, &bytes).unwrap();
                prop_assert_eq!(ps, &restored);
            }
        }
    }
}
