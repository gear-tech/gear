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

pub use ark_bls12_381;
pub use ark_ec;
pub use ark_ff;
pub use ark_scale::{self, ark_serialize};

use super::{BuiltinActorError, BuiltinContext};
use alloc::{vec, vec::Vec};
use ark_bls12_381::Bls12_381;
use ark_ec::pairing::{MillerLoopOutput, Pairing};
use ark_scale::{
    HOST_CALL,
    rw::InputAsRead,
    scale::{Compact, Decode, Encode, Input},
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use gear_core::str::LimitedStr;

const SCALE_USAGE: u8 = ark_scale::make_usage(Compress::No, Validate::No);
type ArkScaleLocal<T> = ark_scale::ArkScale<T, SCALE_USAGE>;

const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

/// Trait for BLS12-381 operations gas cost calculations.
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

/// Copies impl of bls12_381 operations from `sp-crypto-ec-utils` crate
pub struct Bls12_381Ops;

impl Bls12_381Ops {
    pub fn multi_miller_loop(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let a = Self::decode::<Vec<<Bls12_381 as Pairing>::G1Affine>>(g1)?;
        let b = Self::decode::<Vec<<Bls12_381 as Pairing>::G2Affine>>(g2)?;
        let res = Bls12_381::multi_miller_loop(a, b);

        Ok(Self::encode(res.0))
    }

    pub fn final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let f = Self::decode::<<Bls12_381 as Pairing>::TargetField>(f)?;
        let res = Bls12_381::final_exponentiation(MillerLoopOutput(f)).ok_or(
            BuiltinActorError::Custom(LimitedStr::from_small_str("Final exponentiation failed")),
        )?;

        Ok(Self::encode(res.0))
    }

    pub fn encode<T: CanonicalSerialize>(val: T) -> Vec<u8> {
        ArkScaleLocal::from(val).encode()
    }

    fn decode<T: CanonicalDeserialize>(buf: Vec<u8>) -> Result<T, BuiltinActorError> {
        ArkScaleLocal::<T>::decode(&mut &buf[..])
            .map(|v| v.0)
            .map_err(|_| BuiltinActorError::DecodingError)
    }
}

/// Common function for BLS12-381 multi Miller loop operation.
pub fn multi_miller_loop<T: BlsOpsGasCost>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    multi_miller_loop_impl: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>,
) -> Result<Vec<u8>, BuiltinActorError> {
    // TODO: do we need further refactoring here as per #3841?
    let a = decode_vec::<T, _>(&mut payload, context)?;
    let b = decode_vec::<T, _>(&mut payload, context)?;

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

    let gas_cost = T::bls12_381_multi_miller_loop(count as u32);
    context.try_charge_gas(gas_cost)?;

    multi_miller_loop_impl(a, b)
}

pub fn final_exponentiation<T: BlsOpsGasCost>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    final_exponentiation_impl: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>,
) -> Result<Vec<u8>, BuiltinActorError> {
    let f = decode_vec::<T, _>(&mut payload, context)?;

    let to_spend = T::bls12_381_final_exponentiation();
    context.try_charge_gas(to_spend)?;

    final_exponentiation_impl(f)
}

/// Common function for decoding vectors in BLS12-381 operations.
fn decode_vec<T: BlsOpsGasCost, I: Input>(
    input: &mut I,
    context: &mut BuiltinContext,
) -> Result<Vec<u8>, BuiltinActorError> {
    let len = Compact::<u32>::decode(input).map(u32::from).map_err(|_| {
        log::debug!("Failed to scale-decode length of the vector",);
        BuiltinActorError::DecodingError
    })?;

    let to_spend = T::decode_bytes(len);
    context.try_charge_gas(to_spend)?;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();

    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!("Failed to scale-decode vector data",);
        BuiltinActorError::DecodingError
    })
}
