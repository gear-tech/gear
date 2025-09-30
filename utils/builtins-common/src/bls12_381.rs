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
use ark_ec::pairing::Pairing;
pub use ark_ff;
pub use ark_scale::{self, ark_serialize};

use super::{BuiltinContext, BuiltinActorError};
use alloc::{vec, vec::Vec};
use ark_scale::{
    ArkScale,
    HOST_CALL,
    rw::InputAsRead,
    scale::{Input, Encode, Decode, Compact}
};
use ark_bls12_381::Bls12_381;
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize, Compress, Validate};
use gear_core::str::LimitedStr;

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
    pub fn multi_miller_loop(g1: &[u8], g2: &[u8]) -> Result<Vec<u8>, ()> {
        let a = Self::decode::<Vec<<Bls12_381 as Pairing>::G1Affine>>(g1);
        log::warn!("Bls12_381Ops, Decoded a: {:x?}", a);
        let b = Self::decode::<Vec<<Bls12_381 as Pairing>::G2Affine>>(g2);
        log::warn!("Bls12_381Ops, Decoded b: {:x?}", b);

        let a = a?;
        let b = b?;

        let res = Bls12_381::multi_miller_loop(a, b);
        Ok(Self::encode(res.0))
    }

    pub fn encode<T: CanonicalSerialize>(val: T) -> Vec<u8> {
        ArkScale::<T>::from(val).encode()
    }

    fn decode<T: CanonicalDeserialize>(mut buf: &[u8]) -> Result<T, ()> {
        ArkScale::<T>::decode(&mut buf)
            .map(|v| v.0)
            .map_err(|_| ())
    }
}

/// Common function for BLS12-381 multi Miller loop operation.
pub fn multi_miller_loop<T: BlsOpsGasCost, E: Into<BuiltinActorError>>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    multi_miller_loop_impl: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Vec<u8>, E>,
) -> Result<Vec<u8>, BuiltinActorError> {
    // TODO: do we need further refactoring here as per #3841?
    let a = decode_vec::<T, _>(&mut payload, context)?;
    let b = decode_vec::<T, _>(&mut payload, context)?;

    log::warn!("Decoded multi Miller loop inputs: a.len() = {}, b.len() = {}", a.len(), b.len());
    log::warn!("Decoded a: {:x?}", a);
    log::warn!("Decoded b: {:x?}", b);

    // Decode the items count from 'a'
    let mut slice = a.as_slice();
    let mut reader = InputAsRead(&mut slice);
    let count = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED).map_err(|_| {
        log::debug!(
            "Failed to decode items count in a",
        );

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
    context.try_charge_gas(gas_cost).map_err(|_| BuiltinActorError::DecodingError)?;

    multi_miller_loop_impl(a, b).map_err(|e| e.into())
}

/// Common function for decoding vectors in BLS12-381 operations.
fn decode_vec<T: BlsOpsGasCost, I: Input>(
    input: &mut I,
    context: &mut BuiltinContext,
) -> Result<Vec<u8>, BuiltinActorError> {
    let len = Compact::<u32>::decode(input)
        .map(u32::from)
        .map_err(|_| {
            log::debug!(
                "Failed to scale-decode length of the vector",
            );
            BuiltinActorError::DecodingError
        })?;

    let to_spend = T::decode_bytes(len);
    context.try_charge_gas(to_spend)?;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();

    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!(
            "Failed to scale-decode vector data",
        );
        BuiltinActorError::DecodingError
    })
}
