// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};

/// Constant defines codec index of [`Request::MultiMillerLoop`].
pub const REQUEST_MULTI_MILLER_LOOP: u8 = 0;
/// Constant defines codec index of [`Request::FinalExponentiation`].
pub const REQUEST_FINAL_EXPONENTIATION: u8 = 1;
/// Constant defines codec index of [`Request::MultiScalarMultiplicationG1`].
pub const REQUEST_MULTI_SCALAR_MULTIPLICATION_G1: u8 = 2;
/// Constant defines codec index of [`Request::MultiScalarMultiplicationG2`].
pub const REQUEST_MULTI_SCALAR_MULTIPLICATION_G2: u8 = 3;
/// Constant defines codec index of [`Request::ProjectiveMultiplicationG1`].
pub const REQUEST_PROJECTIVE_MULTIPLICATION_G1: u8 = 4;
/// Constant defines codec index of [`Request::ProjectiveMultiplicationG2`].
pub const REQUEST_PROJECTIVE_MULTIPLICATION_G2: u8 = 5;

/// Type that should be used to create a message to the bls12_381 builtin actor.
#[derive(Encode, Clone, PartialEq, Eq, Debug)]
#[codec(crate = codec)]
pub enum Request {
    /// Request to pairing multi Miller loop for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `a`: `ArkScale<Vec<G1Affine>>`.
    ///   - `b`: `ArkScale<Vec<G2Affine>>`.
    #[codec(index = 0)]
    MultiMillerLoop { a: Vec<u8>, b: Vec<u8> },

    /// Request to pairing final exponentiation for *BLS12-381*.
    ///
    /// Encoded: `ArkScale<<Bls12_381::TargetField>`.
    #[codec(index = 1)]
    FinalExponentiation { f: Vec<u8> },

    /// Request to multi scalar multiplication on *G1* for *BLS12-381*
    ///
    /// Encoded:
    ///   - `bases`: `ArkScale<Vec<G1Affine>>`.
    ///   - `scalars`: `ArkScale<Vec<G1Config::ScalarField>>`.
    #[codec(index = 2)]
    MultiScalarMultiplicationG1 { bases: Vec<u8>, scalars: Vec<u8> },

    /// Request to multi scalar multiplication on *G2* for *BLS12-381*
    ///
    /// Encoded:
    ///   - `bases`: `ArkScale<Vec<G2Affine>>`.
    ///   - `scalars`: `ArkScale<Vec<G2Config::ScalarField>>`.
    #[codec(index = 3)]
    MultiScalarMultiplicationG2 { bases: Vec<u8>, scalars: Vec<u8> },

    /// Request to projective multiplication on *G1* for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `base`: `ArkScaleProjective<G1Projective>`.
    ///   - `scalar`: `ArkScale<Vec<u64>>`.
    #[codec(index = 4)]
    ProjectiveMultiplicationG1 { base: Vec<u8>, scalar: Vec<u8> },

    /// Request to projective multiplication on *G2* for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `base`: `ArkScaleProjective<G2Projective>`.
    ///   - `scalar`: `ArkScale<Vec<u64>>`.
    #[codec(index = 5)]
    ProjectiveMultiplicationG2 { base: Vec<u8>, scalar: Vec<u8> },
}

/// The enumeration represents possible common errors for all requests.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
#[codec(crate = codec)]
pub enum Error {
    /// Failed to scale-decode the length of a `Vec<u8>`.
    DecodeVecLength,
    /// Failed to scale-decode bytes.
    DecodeVecData,
    /// Failed to decode the length of a `Vec<G(1,2)Affine>`.
    DecodeItemCount,
}

/// The enumeration contains result to a request.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::From)]
#[codec(crate = codec)]
pub enum Response {
    /// Common error.
    #[from]
    Error(Error),
    /// Result of the multi Miller loop [`MultiMillerLoopResult`].
    #[from]
    MultiMillerLoop(MultiMillerLoopResult),
    /// Result of the final exponentiation.
    FinalExponentiation(Result<Vec<u8>, ()>),
    /// Result of the multi scalar multiplication [`MultiScalarMultiplicationResult`].
    #[from]
    MultiScalarMultiplication(MultiScalarMultiplicationResult),
    /// Result of the projective multiplication.
    ProjectiveMultiplication(Result<Vec<u8>, ()>),
}

/// Result of the multi Miller loop computation.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
#[codec(crate = codec)]
pub enum MultiMillerLoopResult {
    /// Encoded: `ArkScale<Bls12_381::TargetField>`.
    Ok(Vec<u8>),
    /// Computation error.
    Error,
    /// The input lists don't have the same number of items.
    NonEqualItemCount,
}

impl From<Result<Vec<u8>, ()>> for MultiMillerLoopResult {
    fn from(result: Result<Vec<u8>, ()>) -> Self {
        match result {
            Ok(v) => MultiMillerLoopResult::Ok(v),
            Err(_) => MultiMillerLoopResult::Error,
        }
    }
}

/// Result of the multi scalar multiplication.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
#[codec(crate = codec)]
pub enum MultiScalarMultiplicationResult {
    // Encoded: `ArkScaleProjective<ark_bls12_381::G(1,2)Projective>`.
    Ok(Vec<u8>),
    /// Computation error.
    Error,
    /// The input lists don't have the same number of items.
    NonEqualItemCount,
}

impl From<Result<Vec<u8>, ()>> for MultiScalarMultiplicationResult {
    fn from(result: Result<Vec<u8>, ()>) -> Self {
        match result {
            Ok(v) => MultiScalarMultiplicationResult::Ok(v),
            Err(_) => MultiScalarMultiplicationResult::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;

    // The standard Decode implementation cannot be used for precise gas charging.
    // The following test checks that scale codec indexes of variants are set correctly.
    #[test]
    fn codec_enum_indexes() {
        let request = Request::MultiMillerLoop {
            a: vec![],
            b: vec![],
        };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_MULTI_MILLER_LOOP)
        ));

        let request = Request::FinalExponentiation { f: vec![] };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_FINAL_EXPONENTIATION)
        ));

        let request = Request::MultiScalarMultiplicationG1 {
            bases: vec![],
            scalars: vec![],
        };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1)
        ));

        let request = Request::MultiScalarMultiplicationG2 {
            bases: vec![],
            scalars: vec![],
        };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2)
        ));

        let request = Request::ProjectiveMultiplicationG1 {
            base: vec![],
            scalar: vec![],
        };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1)
        ));

        let request = Request::ProjectiveMultiplicationG2 {
            base: vec![],
            scalar: vec![],
        };
        let encoded = request.encode();

        assert!(matches!(
            encoded.first().copied(),
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2)
        ));
    }
}
