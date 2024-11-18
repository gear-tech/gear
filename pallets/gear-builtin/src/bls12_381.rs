// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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
use ark_scale::HOST_CALL;
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use core::marker::PhantomData;
use gbuiltin_bls381::*;
use parity_scale_codec::{Compact, Input};
use sp_crypto_ec_utils::bls12_381;

const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    fn handle(
        dispatch: &StoredDispatch,
        gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let message = dispatch.message();
        let payload = message.payload_bytes();
        let (result, gas_spent) = match payload.first().copied() {
            Some(REQUEST_MULTI_MILLER_LOOP) => multi_miller_loop::<T>(&payload[1..], gas_limit),
            Some(REQUEST_FINAL_EXPONENTIATION) => {
                final_exponentiation::<T>(&payload[1..], gas_limit)
            }
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1) => msm_g1::<T>(&payload[1..], gas_limit),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2) => msm_g2::<T>(&payload[1..], gas_limit),
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1) => {
                projective_multiplication_g1::<T>(&payload[1..], gas_limit)
            }
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2) => {
                projective_multiplication_g2::<T>(&payload[1..], gas_limit)
            }
            Some(REQUEST_AGGREGATE_G1) => aggregate_g1::<T>(&payload[1..], gas_limit),
            Some(REQUEST_MAP_TO_G2AFFINE) => map_to_g2affine::<T>(&payload[1..], gas_limit),
            _ => (Err(BuiltinActorError::DecodingError), 0),
        };

        (
            result.map(|response| {
                response.encode().try_into().unwrap_or_else(|err| {
                    let err_msg = format!(
                        "Actor::handle: Response message is too large. \
                        Response - {response:X?}. Got error - {err:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                })
            }),
            gas_spent,
        )
    }
}

fn decode_vec<T: Config, I: Input>(
    gas_limit: &mut u64,
    input: &mut I,
) -> Result<Vec<u8>, BuiltinActorError> {
    let Ok(len) = Compact::<u32>::decode(input).map(u32::from) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector length"
        );
        return Err(BuiltinActorError::DecodingError);
    };

    let to_spend = <T as Config>::WeightInfo::decode_bytes(len).ref_time();
    if *gas_limit < to_spend {
        return Err(BuiltinActorError::InsufficientGas);
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return Err(BuiltinActorError::GasAllowanceExceeded);
    }

    *gas_limit = gas_limit.saturating_sub(to_spend);

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();

    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector data",
        );

        BuiltinActorError::DecodingError
    })
}

fn multi_miller_loop<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    let mut gas_left: u64 = gas_limit;

    // TODO: do we need further refactorig here as per #3841?
    let a = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    let b = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    // decode the items count
    let mut slice = a.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode items count in a",
        );

        return (
            Err(BuiltinActorError::DecodingError),
            gas_limit.saturating_sub(gas_left),
        );
    };

    let mut slice = b.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return (
                Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "Multi Miller loop: uneven item count",
                ))),
                gas_limit.saturating_sub(gas_left),
            )
        }
        Err(_) => {
            return (
                Err(BuiltinActorError::DecodingError),
                gas_limit.saturating_sub(gas_left),
            )
        }
        Ok(_) => (),
    }

    let to_spend = <T as Config>::WeightInfo::bls12_381_multi_miller_loop(count as u32).ref_time();
    if gas_left < to_spend {
        return (
            Err(BuiltinActorError::InsufficientGas),
            gas_limit.saturating_sub(gas_left),
        );
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (
            Err(BuiltinActorError::GasAllowanceExceeded),
            gas_limit.saturating_sub(gas_left),
        );
    }

    gas_left -= to_spend;

    match bls12_381::host_calls::bls12_381_multi_miller_loop(a, b) {
        Ok(result) => (
            Ok(Response::MultiMillerLoop(result)),
            gas_limit.saturating_sub(gas_left),
        ),
        Err(_) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi Miller loop: computation error",
            ))),
            gas_limit.saturating_sub(gas_left),
        ),
    }
}

fn final_exponentiation<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    let mut gas_left: u64 = gas_limit;
    let f = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    let to_spend = <T as Config>::WeightInfo::bls12_381_final_exponentiation().ref_time();
    if gas_left < to_spend {
        return (
            Err(BuiltinActorError::InsufficientGas),
            gas_limit.saturating_sub(gas_left),
        );
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (
            Err(BuiltinActorError::GasAllowanceExceeded),
            gas_limit.saturating_sub(gas_left),
        );
    }

    gas_left -= to_spend;

    match bls12_381::host_calls::bls12_381_final_exponentiation(f) {
        Ok(result) => (
            Ok(Response::FinalExponentiation(result)),
            gas_limit.saturating_sub(gas_left),
        ),
        Err(_) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Final exponentiation: computation error",
            ))),
            gas_limit.saturating_sub(gas_left),
        ),
    }
}

fn msm<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> (Result<Response, BuiltinActorError>, u64) {
    let mut gas_left: u64 = gas_limit;

    let bases = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    let scalars = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    // decode the count of items
    let mut slice = bases.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode items count in bases",
        );

        return (
            Err(BuiltinActorError::DecodingError),
            gas_limit.saturating_sub(gas_left),
        );
    };

    let mut slice = scalars.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return (
                Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "Multi scalar multiplication: uneven item count",
                ))),
                gas_limit.saturating_sub(gas_left),
            )
        }
        Err(_) => {
            log::debug!(
                target: LOG_TARGET,
                "Failed to decode items count in scalars",
            );

            return (
                Err(BuiltinActorError::DecodingError),
                gas_limit.saturating_sub(gas_left),
            );
        }
        Ok(_) => (),
    }

    let to_spend = gas_to_spend(count as u32);
    if gas_left < to_spend {
        return (
            Err(BuiltinActorError::InsufficientGas),
            gas_limit.saturating_sub(gas_left),
        );
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (
            Err(BuiltinActorError::GasAllowanceExceeded),
            gas_limit.saturating_sub(gas_left),
        );
    }

    gas_left -= to_spend;

    match call(bases, scalars) {
        Ok(result) => (Ok(result), gas_limit.saturating_sub(gas_left)),
        Err(_) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi scalar multiplication: computation error",
            ))),
            gas_limit.saturating_sub(gas_left),
        ),
    }
}

fn msm_g1<T: Config>(payload: &[u8], gas_limit: u64) -> (Result<Response, BuiltinActorError>, u64) {
    msm::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g1(count).ref_time(),
        |bases, scalars| {
            bls12_381::host_calls::bls12_381_msm_g1(bases, scalars)
                .map(Response::MultiScalarMultiplicationG1)
        },
    )
}

fn msm_g2<T: Config>(payload: &[u8], gas_limit: u64) -> (Result<Response, BuiltinActorError>, u64) {
    msm::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g2(count).ref_time(),
        |bases, scalars| {
            bls12_381::host_calls::bls12_381_msm_g2(bases, scalars)
                .map(Response::MultiScalarMultiplicationG2)
        },
    )
}

fn projective_multiplication<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> (Result<Response, BuiltinActorError>, u64) {
    let mut gas_left: u64 = gas_limit;

    let base = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    let scalar = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    // decode the count of items
    let mut slice = scalar.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode items count in scalar",
        );

        return (
            Err(BuiltinActorError::DecodingError),
            gas_limit.saturating_sub(gas_left),
        );
    };

    let to_spend = gas_to_spend(count as u32);
    if gas_limit < to_spend {
        return (
            Err(BuiltinActorError::InsufficientGas),
            gas_limit.saturating_sub(gas_left),
        );
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (
            Err(BuiltinActorError::GasAllowanceExceeded),
            gas_limit.saturating_sub(gas_left),
        );
    }

    gas_left -= to_spend;

    match call(base, scalar) {
        Ok(result) => (Ok(result), gas_limit.saturating_sub(gas_left)),
        Err(_) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Projective multiplication: computation error",
            ))),
            gas_limit.saturating_sub(gas_left),
        ),
    }
}

fn projective_multiplication_g1<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    projective_multiplication::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g1(count).ref_time(),
        |base, scalar| {
            bls12_381::host_calls::bls12_381_mul_projective_g1(base, scalar)
                .map(Response::ProjectiveMultiplicationG1)
        },
    )
}

fn projective_multiplication_g2<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    projective_multiplication::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g2(count).ref_time(),
        |base, scalar| {
            bls12_381::host_calls::bls12_381_mul_projective_g2(base, scalar)
                .map(Response::ProjectiveMultiplicationG2)
        },
    )
}

fn aggregate_g1<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    let mut gas_left: u64 = gas_limit;

    let points = match decode_vec::<T, _>(&mut gas_left, &mut payload) {
        Ok(array) => array,
        Err(e) => return (Err(e), gas_limit.saturating_sub(gas_left)),
    };

    // decode the count of items
    let mut slice = points.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode items count in points",
        );

        return (
            Err(BuiltinActorError::DecodingError),
            gas_limit.saturating_sub(gas_left),
        );
    };

    let to_spend = <T as Config>::WeightInfo::bls12_381_aggregate_g1(count as u32).ref_time();
    if gas_limit < to_spend {
        return (
            Err(BuiltinActorError::InsufficientGas),
            gas_limit.saturating_sub(gas_left),
        );
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (
            Err(BuiltinActorError::GasAllowanceExceeded),
            gas_limit.saturating_sub(gas_left),
        );
    }

    gas_left -= to_spend;

    (
        gear_runtime_interface::gear_bls_12_381::aggregate_g1(&points)
            .map(Response::AggregateG1)
            .map_err(|e| {
                log::debug!(
                    target: LOG_TARGET,
                    "Failed to aggregate G1-points: {e}"
                );

                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "Aggregate G1-points: computation error",
                ))
            }),
        gas_limit.saturating_sub(gas_left),
    )
}

fn map_to_g2affine<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (Result<Response, BuiltinActorError>, u64) {
    let Ok(len) = Compact::<u32>::decode(&mut payload).map(u32::from) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector length"
        );
        return (Err(BuiltinActorError::DecodingError), 0);
    };

    if len != payload.len() as u32 {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector length"
        );

        return (Err(BuiltinActorError::DecodingError), 0);
    }

    let to_spend = <T as Config>::WeightInfo::bls12_381_map_to_g2affine(len).ref_time();
    if gas_limit < to_spend {
        return (Err(BuiltinActorError::InsufficientGas), 0);
    }
    if GasAllowanceOf::<T>::get() < to_spend {
        return (Err(BuiltinActorError::GasAllowanceExceeded), 0);
    }

    (
        gear_runtime_interface::gear_bls_12_381::map_to_g2affine(payload)
            .map(Response::MapToG2Affine)
            .map_err(|e| {
                log::debug!(
                    target: LOG_TARGET,
                    "Failed to map a message: {e}"
                );

                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "Mapping message: computation error",
                ))
            }),
        to_spend,
    )
}
