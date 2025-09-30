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

use super::*;
// todo [sab] remove this deps
use ark_scale::HOST_CALL;
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use core::marker::PhantomData;
use gbuiltin_bls381::*;
use gear_runtime_interface::gear_bls_12_381 as ri_gear_bls_12_381;
use builtins_common::bls12_381::{self as gear_bls_12_381_ops, BlsOpsGasCost};
use parity_scale_codec::{Compact, Input};
use sp_crypto_ec_utils::bls12_381 as ri_sp_bls12_381;

const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

struct RiGearBls12_381ErrorCodeConverter(u32);

impl From<RiGearBls12_381ErrorCodeConverter> for BuiltinActorError {
    fn from(err: RiGearBls12_381ErrorCodeConverter) -> Self {
        use gear_runtime_interface::GearBls12_381Error;

        match err.0.into() {
            GearBls12_381Error::Decode => BuiltinActorError::DecodingError,
            GearBls12_381Error::EmptyPointList => {
                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "BLS12-381: empty point list",
                ))
            }
            GearBls12_381Error::MapperCreation => {
                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "BLS12-381: mapper creation error",
                ))
            }
            GearBls12_381Error::MessageMapping => {
                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "BLS12-381: message mapping error",
                ))
            }
        }
    }
}

enum RiGearBls12_381Call {
    MultiMillerLoop(Vec<u8>, Vec<u8>),
}

impl RiGearBls12_381Call {
    fn execute<T: Config>(self) -> Result<Vec<u8>, BuiltinActorError> {
        match self {
            RiGearBls12_381Call::MultiMillerLoop(a, b) => {
                ri_gear_bls_12_381::multi_miller_loop(&a, &b)
                    .map_err(|e| RiGearBls12_381ErrorCodeConverter(e).into())
            }
        }
    }
}

struct Bls12_381OpsImpl<T: Config>(PhantomData<T>);

impl<T: Config> BlsOpsGasCost for Bls12_381OpsImpl<T> {
    fn decode_bytes(len: u32) -> u64 {
        <T as Config>::WeightInfo::decode_bytes(len).ref_time()
    }

    fn bls12_381_multi_miller_loop(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_multi_miller_loop(count).ref_time()
    }

    fn bls12_381_final_exponentiation() -> u64 {
        <T as Config>::WeightInfo::bls12_381_final_exponentiation().ref_time()
    }

    fn bls12_381_msm_g1(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_msm_g1(count).ref_time()
    }

    fn bls12_381_msm_g2(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_msm_g2(count).ref_time()
    }

    fn bls12_381_mul_projective_g1(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_mul_projective_g1(count).ref_time()
    }

    fn bls12_381_mul_projective_g2(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_mul_projective_g2(count).ref_time()
    }

    fn bls12_381_aggregate_g1(count: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_aggregate_g1(count).ref_time()
    }

    fn bls12_381_map_to_g2affine(len: u32) -> u64 {
        <T as Config>::WeightInfo::bls12_381_map_to_g2affine(len).ref_time()
    }
}

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        let message = dispatch.message();
        let payload = message.payload_bytes();
        log::warn!("Received payload: {:?}", payload);
        match payload.first().copied() {
            Some(REQUEST_MULTI_MILLER_LOOP) => multi_miller_loop::<T>(&payload[1..], context),
            Some(REQUEST_FINAL_EXPONENTIATION) => final_exponentiation::<T>(&payload[1..], context),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1) => msm_g1::<T>(&payload[1..], context),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2) => msm_g2::<T>(&payload[1..], context),
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1) => {
                projective_multiplication_g1::<T>(&payload[1..], context)
            }
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2) => {
                projective_multiplication_g2::<T>(&payload[1..], context)
            }
            Some(REQUEST_AGGREGATE_G1) => aggregate_g1::<T>(&payload[1..], context),
            Some(REQUEST_MAP_TO_G2AFFINE) => map_to_g2affine::<T>(&payload[1..], context),
            _ => Err(BuiltinActorError::DecodingError),
        }
        .map(|response| BuiltinReply {
            payload: response.encode().try_into().unwrap_or_else(|err| {
                let err_msg = format!(
                    "Actor::handle: Response message is too large. \
                        Response - {response:X?}. Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }),
            // The value is not used in the bls12_381 actor, it will be fully returned to the caller.
            value: dispatch.value(),
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

fn multi_miller_loop<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    log::warn!("Received multi-miller payload: {:?}", payload);
    gear_bls_12_381_ops::multi_miller_loop::<Bls12_381OpsImpl<T>, _>(
        payload,
        context,
        |a, b| RiGearBls12_381Call::MultiMillerLoop(a, b).execute::<T>()
    )
    .map(Response::MultiMillerLoop)
}

fn decode_vec<T: Config, I: Input>(
    input: &mut I,
    context: &mut BuiltinContext,
) -> Result<Vec<u8>, BuiltinActorError> {
    let len = Compact::<u32>::decode(input).map(u32::from).map_err(|_| {
        log::debug!(
            "Failed to scale-decode vector length"
        );
        BuiltinActorError::DecodingError
    })?;

    let to_spend = <T as Config>::WeightInfo::decode_bytes(len).ref_time();
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

fn final_exponentiation<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let f = decode_vec::<T, _>(&mut payload, context)?;

    let to_spend = <T as Config>::WeightInfo::bls12_381_final_exponentiation().ref_time();
    context.try_charge_gas(to_spend)?;

    match ri_sp_bls12_381::host_calls::bls12_381_final_exponentiation(f) {
        Ok(result) => Ok(Response::FinalExponentiation(result)),
        Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Final exponentiation: computation error",
        ))),
    }
}

fn msm<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> Result<Response, BuiltinActorError> {
    let bases = decode_vec::<T, _>(&mut payload, context)?;
    let scalars = decode_vec::<T, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = bases.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            "Failed to decode items count in bases",
        );

        return Err(BuiltinActorError::DecodingError);
    };

    let mut slice = scalars.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi scalar multiplication: uneven item count",
            )));
        }
        Err(_) => {
            log::debug!(
                "Failed to decode items count in scalars",
            );

            return Err(BuiltinActorError::DecodingError);
        }
        Ok(_) => (),
    }

    let to_spend = gas_to_spend(count as u32);
    context.try_charge_gas(to_spend)?;

    match call(bases, scalars) {
        Ok(result) => Ok(result),
        Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Multi scalar multiplication: computation error",
        ))),
    }
}

fn msm_g1<T: Config>(
    payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    msm::<T>(
        payload,
        context,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g1(count).ref_time(),
        |bases, scalars| {
            ri_sp_bls12_381::host_calls::bls12_381_msm_g1(bases, scalars)
                .map(Response::MultiScalarMultiplicationG1)
        },
    )
}

fn msm_g2<T: Config>(
    payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    msm::<T>(
        payload,
        context,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g2(count).ref_time(),
        |bases, scalars| {
            ri_sp_bls12_381::host_calls::bls12_381_msm_g2(bases, scalars)
                .map(Response::MultiScalarMultiplicationG2)
        },
    )
}

fn projective_multiplication<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> Result<Response, BuiltinActorError> {
    let base = decode_vec::<T, _>(&mut payload, context)?;
    let scalar = decode_vec::<T, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = scalar.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            "Failed to decode items count in scalar",
        );

        return Err(BuiltinActorError::DecodingError);
    };

    let to_spend = gas_to_spend(count as u32);
    context.try_charge_gas(to_spend)?;

    call(base, scalar).map_err(|_| {
        BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Projective multiplication: computation error",
        ))
    })
}

fn projective_multiplication_g1<T: Config>(
    payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    projective_multiplication::<T>(
        payload,
        context,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g1(count).ref_time(),
        |base, scalar| {
            ri_sp_bls12_381::host_calls::bls12_381_mul_projective_g1(base, scalar)
                .map(Response::ProjectiveMultiplicationG1)
        },
    )
}

fn projective_multiplication_g2<T: Config>(
    payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    projective_multiplication::<T>(
        payload,
        context,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g2(count).ref_time(),
        |base, scalar| {
            ri_sp_bls12_381::host_calls::bls12_381_mul_projective_g2(base, scalar)
                .map(Response::ProjectiveMultiplicationG2)
        },
    )
}

fn aggregate_g1<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let points = decode_vec::<T, _>(&mut payload, context)?;

    // decode the count of items
    let mut slice = points.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            "Failed to decode items count in points",
        );

        return Err(BuiltinActorError::DecodingError);
    };

    let to_spend = <T as Config>::WeightInfo::bls12_381_aggregate_g1(count as u32).ref_time();
    context.try_charge_gas(to_spend)?;

    gear_runtime_interface::gear_bls_12_381::aggregate_g1(&points)
        .map(Response::AggregateG1)
        .map_err(|e| {
            log::debug!(
                "Failed to aggregate G1-points: {e}"
            );

            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Aggregate G1-points: computation error",
            ))
        })
}

fn map_to_g2affine<T: Config>(
    mut payload: &[u8],
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let len = Compact::<u32>::decode(&mut payload)
        .map(u32::from)
        .map_err(|_| {
            log::debug!(
                "Failed to scale-decode vector length"
            );
            BuiltinActorError::DecodingError
        })?;

    if len != payload.len() as u32 {
        log::debug!(
            "Failed to scale-decode vector length"
        );

        return Err(BuiltinActorError::DecodingError);
    }

    let to_spend = <T as Config>::WeightInfo::bls12_381_map_to_g2affine(len).ref_time();
    context.try_charge_gas(to_spend)?;

    gear_runtime_interface::gear_bls_12_381::map_to_g2affine(payload)
        .map(Response::MapToG2Affine)
        .map_err(|e| {
            log::debug!(
                "Failed to map a message: {e}"
            );

            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Mapping message: computation error",
            ))
        })
}
