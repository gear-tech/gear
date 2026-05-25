// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{DealerOutput, MasterSecretKey, Result, SecretKeyShare, TpkeError};

use ark_bls12_381::Fr;
use ark_poly::{DenseUVPolynomial, Polynomial, univariate::DensePolynomial};
use ark_std::rand::{CryptoRng, RngCore};

/// Run the dealer ceremony locally: sample a fresh master secret, split
/// it into `n` Shamir shares with threshold `t`, and return shares + pubs.
pub fn deal<R: RngCore + CryptoRng>(t: u32, n: u32, rng: &mut R) -> Result<DealerOutput> {
    if t == 0 || n == 0 || t > n {
        return Err(TpkeError::InvalidThreshold { t, n });
    }

    // polynomial degree is `threshold - 1`
    let polynomial = DensePolynomial::<Fr>::rand(t as usize - 1, rng);

    let (public_shares, secret_shares) = (1..=n)
        .map(|index| {
            let point = Fr::from(index as u64);
            let res = polynomial.evaluate(&point);
            let secret_share = SecretKeyShare::new(index, res);
            (secret_share.to_public(), secret_share)
        })
        .collect::<(Vec<_>, Vec<_>)>();

    let secret = polynomial.coeffs().first().copied().unwrap();
    let master_pub = MasterSecretKey::new(secret).to_public();

    Ok(DealerOutput {
        master_pub,
        secret_shares,
        public_shares,
    })
}
