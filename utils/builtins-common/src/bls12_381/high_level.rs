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

use super::*;
use ark_scale::{
    HOST_CALL,
    rw::InputAsRead,
    scale::{Compact, Decode, Input},
};
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use gear_core::str::LimitedStr;

const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

/// High-level BLS12-381 Multi-miller loop op hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn multi_miller_loop<Gas: BlsOpsGasCost, Ops: Bls12_381Ops>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    // TODO: do we need further refactoring here as per #3841?
    let a = decode_vec::<Gas, _>(&mut payload, context)?;
    let b = decode_vec::<Gas, _>(&mut payload, context)?;

    // Decode the items count from 'a'
    let mut slice = a.as_slice();
    let mut reader = InputAsRead(&mut slice);
    let count =
        u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED).map_err(|_| {
            log::debug!("Failed to decode items count in a",);

            BuiltinActorError::DecodingError
        })?;

    // Decode the items count from 'b' and verify they match
    let mut slice = b.as_slice();
    let mut reader = InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi Miller loop: uneven item count",
            )));
        }
        Err(_) => return Err(BuiltinActorError::DecodingError),
        Ok(_) => (),
    }

    let gas_cost = Gas::bls12_381_multi_miller_loop(count as u32);
    context.try_charge_gas(gas_cost)?;

    Ops::multi_miller_loop(a, b).map(Response::MultiMillerLoop)
}

/// High-level BLS12-381 Final exponentiation op hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn final_exponentiation<Gas: BlsOpsGasCost, Ops: Bls12_381Ops>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let f = decode_vec::<Gas, _>(&mut payload, context)?;

    let to_spend = Gas::bls12_381_final_exponentiation();
    context.try_charge_gas(to_spend)?;

    Ops::final_exponentiation(f).map(Response::FinalExponentiation)
}

/// High-level BLS12-381 Multi-scalar multiplication ops hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn msm<Gas: BlsOpsGasCost>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    gas_cost_fn: impl FnOnce(u32) -> u64,
    msm_impl: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, BuiltinActorError>,
) -> Result<Response, BuiltinActorError> {
    let bases = decode_vec::<Gas, _>(&mut payload, context)?;
    let scalars = decode_vec::<Gas, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = bases.as_slice();
    let mut reader = InputAsRead(&mut slice);
    let count =
        u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED).map_err(|_| {
            log::debug!("Failed to decode items count in bases");

            BuiltinActorError::DecodingError
        })?;

    let mut slice = scalars.as_slice();
    let mut reader = InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi scalar multiplication: uneven item count",
            )));
        }
        Err(_) => {
            log::debug!("Failed to decode items count in scalars");

            return Err(BuiltinActorError::DecodingError);
        }
        Ok(_) => (),
    }

    let to_spend = gas_cost_fn(count as u32);
    context.try_charge_gas(to_spend)?;

    msm_impl(bases, scalars)
}

/// High-level BLS12-381 Projective multiplication ops hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn projective_multiplication<Gas: BlsOpsGasCost>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    gas_cost_fn: impl FnOnce(u32) -> u64,
    mul_impl: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, BuiltinActorError>,
) -> Result<Response, BuiltinActorError> {
    let base = decode_vec::<Gas, _>(&mut payload, context)?;
    let scalar = decode_vec::<Gas, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = scalar.as_slice();
    let mut reader = InputAsRead(&mut slice);
    let count =
        u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED).map_err(|_| {
            log::debug!("Failed to decode count of items in scalar");

            BuiltinActorError::DecodingError
        })?;

    let to_spend = gas_cost_fn(count as u32);
    context.try_charge_gas(to_spend)?;

    mul_impl(base, scalar)
}

/// High-level BLS12-381 G1 aggregation op hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn aggregate_g1<Gas: BlsOpsGasCost, Ops: Bls12_381Ops>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let points = decode_vec::<Gas, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = points.as_slice();
    let mut reader = InputAsRead(&mut slice);
    let count =
        u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED).map_err(|_| {
            log::debug!("Failed to decode count of items in points");

            BuiltinActorError::DecodingError
        })?;

    let to_spend = Gas::bls12_381_aggregate_g1(count as u32);
    context.try_charge_gas(to_spend)?;

    Ops::aggregate_g1(points).map(Response::AggregateG1)
}

/// High-level BLS12-381 Map to G2Affine op hiding under `Bls12_381Ops` trait
/// parameter actual implementation details. For more info about the abstraction
/// requirement see the `Bls12_381Ops` trait documentation.
///
/// The function performs preparation steps to call the actual implementation.
/// Also it performs gas charging using the provided `Gas` trait parameter.
pub fn map_to_g2affine<Gas: BlsOpsGasCost, Ops: Bls12_381Ops>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let len = Compact::<u32>::decode(&mut payload)
        .map(u32::from)
        .map_err(|_| BuiltinActorError::DecodingError)?;

    if len != payload.len() as u32 {
        return Err(BuiltinActorError::DecodingError);
    }

    let to_spend = Gas::bls12_381_map_to_g2affine(len);
    context.try_charge_gas(to_spend)?;

    Ops::map_to_g2affine(payload.to_vec()).map(Response::MapToG2Affine)
}

fn decode_vec<Gas: BlsOpsGasCost, I: Input>(
    input: &mut I,
    context: &mut BuiltinContext,
) -> Result<Vec<u8>, BuiltinActorError> {
    let len = Compact::<u32>::decode(input).map(u32::from).map_err(|_| {
        log::debug!("Failed to scale-decode length of the vector",);
        BuiltinActorError::DecodingError
    })?;

    let to_spend = Gas::decode_bytes(len);
    context.try_charge_gas(to_spend)?;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();

    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!("Failed to scale-decode vector data",);
        BuiltinActorError::DecodingError
    })
}
