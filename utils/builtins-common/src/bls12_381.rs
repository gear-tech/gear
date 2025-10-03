// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

mod high_level;
mod low_level;

pub use gbuiltin_bls381::{
    Request, Response, ark_bls12_381, ark_ec, ark_ff, ark_scale, ark_serialize,
};
pub use high_level::*;
pub use low_level::Bls12_381OpsLowLevel;

use super::{BuiltinActorError, BuiltinContext};
use alloc::{vec, vec::Vec};
use gbuiltin_bls381::{
    REQUEST_AGGREGATE_G1, REQUEST_FINAL_EXPONENTIATION, REQUEST_MAP_TO_G2AFFINE,
    REQUEST_MULTI_MILLER_LOOP, REQUEST_MULTI_SCALAR_MULTIPLICATION_G1,
    REQUEST_MULTI_SCALAR_MULTIPLICATION_G2, REQUEST_PROJECTIVE_MULTIPLICATION_G1,
    REQUEST_PROJECTIVE_MULTIPLICATION_G2,
};

/// Executes BLS12-381 built-in functions.
///
/// Checks the first byte of the input to determine which BLS12-381 operation to perform,
/// and then calls the corresponding function with the remaining input bytes.
pub fn execute_bls12_381_builtins<Gas: BlsOpsGasCost, Ops: Bls12_381Ops>(
    input: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    match input.first().copied() {
        Some(REQUEST_MULTI_MILLER_LOOP) => {
            high_level::multi_miller_loop::<Gas, Ops>(&input[1..], context)
        }
        Some(REQUEST_FINAL_EXPONENTIATION) => {
            high_level::final_exponentiation::<Gas, Ops>(&input[1..], context)
        }
        Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1) => high_level::msm::<Gas>(
            &input[1..],
            context,
            Gas::bls12_381_msm_g1,
            |bases, scalars| Ops::msm_g1(bases, scalars).map(Response::MultiScalarMultiplicationG1),
        ),
        Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2) => high_level::msm::<Gas>(
            &input[1..],
            context,
            Gas::bls12_381_msm_g2,
            |bases, scalars| Ops::msm_g2(bases, scalars).map(Response::MultiScalarMultiplicationG2),
        ),
        Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1) => high_level::projective_multiplication::<Gas>(
            &input[1..],
            context,
            Gas::bls12_381_mul_projective_g1,
            |base, scalar| {
                Ops::projective_mul_g1(base, scalar).map(Response::ProjectiveMultiplicationG1)
            },
        ),
        Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2) => high_level::projective_multiplication::<Gas>(
            &input[1..],
            context,
            Gas::bls12_381_mul_projective_g2,
            |base, scalar| {
                Ops::projective_mul_g2(base, scalar).map(Response::ProjectiveMultiplicationG2)
            },
        ),
        Some(REQUEST_AGGREGATE_G1) => high_level::aggregate_g1::<Gas, Ops>(&input[1..], context),
        Some(REQUEST_MAP_TO_G2AFFINE) => {
            high_level::map_to_g2affine::<Gas, Ops>(&input[1..], context)
        }
        _ => Err(BuiltinActorError::DecodingError),
    }
}

/// BLS12-381 operations gas cost trait.
///
/// Depending on the environment (e.g., runtime or tests), different values for gas costs
/// can be provided by implementing this trait accordingly.
pub trait BlsOpsGasCost {
    /// Returns gas cost for decoding bytes.
    fn decode_bytes(len: u32) -> u64;
    /// Returns gas cost for BLS12-381 multi Miller loop operation.
    fn bls12_381_multi_miller_loop(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 final exponentiation operation.
    fn bls12_381_final_exponentiation() -> u64;
    /// Returns gas cost for BLS12-381 MSM G1 operation.
    fn bls12_381_msm_g1(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 MSM G2 operation.
    fn bls12_381_msm_g2(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 projective multiplication G1 operation.
    fn bls12_381_mul_projective_g1(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 projective multiplication G2 operation.
    fn bls12_381_mul_projective_g2(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 G1 aggregation operation.
    fn bls12_381_aggregate_g1(count: u32) -> u64;
    /// Returns gas cost for BLS12-381 map to G2Affine operation.
    fn bls12_381_map_to_g2affine(len: u32) -> u64;
}
/// Bls12-381 operations trait.
///
/// The trait abstracts the actual implementation of BLS12-381 operations. Depending
/// on the environment (e.g., runtime or tests), bls operations can be implemented
/// as host calls from the runtime, or directly using the `ark`s crates.
pub trait Bls12_381Ops {
    /// Performs the multi Miller loop operation on pairs of G1 and G2 points.
    fn multi_miller_loop(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the final exponentiation operation.
    fn final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the multi-scalar multiplication operation on G1 points.
    fn msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the multi-scalar multiplication operation on G2 points.
    fn msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the projective multiplication operation on G1 points.
    fn projective_mul_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the projective multiplication operation on G2 points.
    fn projective_mul_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Performs the aggregation operation on G1 points.
    fn aggregate_g1(points: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
    /// Maps a message to a G2Affine point.
    fn map_to_g2affine(message: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>;
}
