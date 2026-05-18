// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ark_bls12_381::{Fr, G2Affine};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{Field, UniformRand, Zero};
use ark_std::rand::{CryptoRng, RngCore};
use zeroize::Zeroize;

use crate::{
    DealerOutput, MasterPublicKey, MasterSecretKey, SecretKeyShare, SharePublicKey, TpkeError,
    TpkeResult,
};

impl MasterSecretKey {
    /// Run the dealer ceremony locally: sample a fresh master secret, split
    /// it into `n` Shamir shares with threshold `t`, and return shares + pubs.
    ///
    /// `t` is the number of shares required to decrypt. `n` is the total
    /// validator count. Indices in the returned shares are 1..=n.
    ///
    /// The returned `MasterSecretKey` SHOULD be zeroized by the caller as soon
    /// as the shares are persisted off-machine (`drop` does this on Drop).
    pub fn deal<R: RngCore + CryptoRng>(t: u32, n: u32, rng: &mut R) -> TpkeResult<DealerOutput> {
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

/// Compute Lagrange coefficient `λᵢ = ∏_{j != i} (j / (j - i))` evaluated at 0.
///
/// Indices are 1-based validator ids. Returns `None` if any (j - i) is zero
/// (caller must dedupe before calling).
pub(crate) fn lagrange_coefficient(i: u32, indices: &[u32]) -> Option<Fr> {
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
