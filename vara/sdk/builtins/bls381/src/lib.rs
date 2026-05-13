// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use ark_bls12_381;
pub use ark_ec;
pub use ark_ff;
pub use ark_scale;
pub use ark_serialize;

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
/// Constant defines codec index of [`Request::AggregateG1`].
pub const REQUEST_AGGREGATE_G1: u8 = 6;
/// Constant defines codec index of [`Request::MapToG2Affine`].
pub const REQUEST_MAP_TO_G2AFFINE: u8 = 7;

/// Type that should be used to create a message to the bls12_381 builtin actor.
/// Use the following crates to construct a request:
///  - `ark-scale`: <https://docs.rs/ark-scale/>;
///  - `ark-bls12-381`: <https://docs.rs/ark-bls12-381/>.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub enum Request {
    /// Request to pairing multi Miller loop for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `a`: [`ArkScale<Vec<G1Affine>>`](https://docs.rs/ark-scale/).
    ///   - `b`: [`ArkScale<Vec<G2Affine>>`](https://docs.rs/ark-scale/).
    #[codec(index = 0)]
    MultiMillerLoop { a: Vec<u8>, b: Vec<u8> },

    /// Request to pairing final exponentiation for *BLS12-381*.
    ///
    /// Encoded: [`ArkScale<<Bls12_381::TargetField>`](https://docs.rs/ark-scale/).
    #[codec(index = 1)]
    FinalExponentiation { f: Vec<u8> },

    /// Request to multi scalar multiplication on *G1* for *BLS12-381*
    ///
    /// Encoded:
    ///   - `bases`: [`ArkScale<Vec<G1Affine>>`](https://docs.rs/ark-scale/).
    ///   - `scalars`: [`ArkScale<Vec<G1Config::ScalarField>>`](https://docs.rs/ark-scale/).
    #[codec(index = 2)]
    MultiScalarMultiplicationG1 { bases: Vec<u8>, scalars: Vec<u8> },

    /// Request to multi scalar multiplication on *G2* for *BLS12-381*
    ///
    /// Encoded:
    ///   - `bases`: [`ArkScale<Vec<G2Affine>>`](https://docs.rs/ark-scale/).
    ///   - `scalars`: [`ArkScale<Vec<G2Config::ScalarField>>`](https://docs.rs/ark-scale/).
    #[codec(index = 3)]
    MultiScalarMultiplicationG2 { bases: Vec<u8>, scalars: Vec<u8> },

    /// Request to projective multiplication on *G1* for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `base`: [`ArkScaleProjective<G1Projective>`](https://docs.rs/ark-scale/).
    ///   - `scalar`: [`ArkScale<Vec<u64>>`](https://docs.rs/ark-scale/).
    #[codec(index = 4)]
    ProjectiveMultiplicationG1 { base: Vec<u8>, scalar: Vec<u8> },

    /// Request to projective multiplication on *G2* for *BLS12-381*.
    ///
    /// Encoded:
    ///   - `base`: [`ArkScaleProjective<G2Projective>`](https://docs.rs/ark-scale/).
    ///   - `scalar`: [`ArkScale<Vec<u64>>`](https://docs.rs/ark-scale/).
    #[codec(index = 5)]
    ProjectiveMultiplicationG2 { base: Vec<u8>, scalar: Vec<u8> },

    /// Request to aggregate *G1* points for *BLS12-381*.
    ///
    /// Encoded: [`ArkScale<Vec<G1Projective>>`](https://docs.rs/ark-scale/).
    #[codec(index = 6)]
    AggregateG1 { points: Vec<u8> },

    /// Request to map an arbitrary message to *G2Affine* point for *BLS12-381*.
    ///
    /// Raw message bytes to map.
    #[codec(index = 7)]
    MapToG2Affine { message: Vec<u8> },
}

/// The enumeration contains result to a request.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub enum Response {
    /// Result of the multi Miller loop, encoded: [`ArkScale<Bls12_381::TargetField>`](https://docs.rs/ark-scale/).
    #[codec(index = 0)]
    MultiMillerLoop(Vec<u8>),
    /// Result of the final exponentiation, encoded: [`ArkScale<Bls12_381::TargetField>`](https://docs.rs/ark-scale/).
    #[codec(index = 1)]
    FinalExponentiation(Vec<u8>),
    /// Result of the multi scalar multiplication, encoded: [`ArkScaleProjective<G1Projective>`](https://docs.rs/ark-scale/).
    #[codec(index = 2)]
    MultiScalarMultiplicationG1(Vec<u8>),
    /// Result of the multi scalar multiplication, encoded: [`ArkScaleProjective<G2Projective>`](https://docs.rs/ark-scale/).
    #[codec(index = 3)]
    MultiScalarMultiplicationG2(Vec<u8>),
    /// Result of the projective multiplication, encoded: [`ArkScaleProjective<G1Projective>`](https://docs.rs/ark-scale/).
    #[codec(index = 4)]
    ProjectiveMultiplicationG1(Vec<u8>),
    /// Result of the projective multiplication, encoded: [`ArkScaleProjective<G2Projective>`](https://docs.rs/ark-scale/).
    #[codec(index = 5)]
    ProjectiveMultiplicationG2(Vec<u8>),
    /// Result of the aggregation, encoded: [`ArkScale<G1Projective>`](https://docs.rs/ark-scale/).
    #[codec(index = 6)]
    AggregateG1(Vec<u8>),
    /// Result of the mapping, encoded: [`ArkScale<G2Affine>`](https://docs.rs/ark-scale/).
    #[codec(index = 7)]
    MapToG2Affine(Vec<u8>),
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
        for (index, (variant, request, response)) in [
            (
                REQUEST_MULTI_MILLER_LOOP,
                Request::MultiMillerLoop {
                    a: vec![],
                    b: vec![],
                },
                Response::MultiMillerLoop(vec![]),
            ),
            (
                REQUEST_FINAL_EXPONENTIATION,
                Request::FinalExponentiation { f: vec![] },
                Response::FinalExponentiation(vec![]),
            ),
            (
                REQUEST_MULTI_SCALAR_MULTIPLICATION_G1,
                Request::MultiScalarMultiplicationG1 {
                    bases: vec![],
                    scalars: vec![],
                },
                Response::MultiScalarMultiplicationG1(vec![]),
            ),
            (
                REQUEST_MULTI_SCALAR_MULTIPLICATION_G2,
                Request::MultiScalarMultiplicationG2 {
                    bases: vec![],
                    scalars: vec![],
                },
                Response::MultiScalarMultiplicationG2(vec![]),
            ),
            (
                REQUEST_PROJECTIVE_MULTIPLICATION_G1,
                Request::ProjectiveMultiplicationG1 {
                    base: vec![],
                    scalar: vec![],
                },
                Response::ProjectiveMultiplicationG1(vec![]),
            ),
            (
                REQUEST_PROJECTIVE_MULTIPLICATION_G2,
                Request::ProjectiveMultiplicationG2 {
                    base: vec![],
                    scalar: vec![],
                },
                Response::ProjectiveMultiplicationG2(vec![]),
            ),
            (
                REQUEST_AGGREGATE_G1,
                Request::AggregateG1 { points: vec![] },
                Response::AggregateG1(vec![]),
            ),
            (
                REQUEST_MAP_TO_G2AFFINE,
                Request::MapToG2Affine { message: vec![] },
                Response::MapToG2Affine(vec![]),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(index, variant.into());

            let request = request.encode();
            assert!(matches!(request.first().copied(), Some(v) if v == variant));

            let response = response.encode();
            assert!(matches!(response.first().copied(), Some(v) if v == variant));
        }
    }
}
