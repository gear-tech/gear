// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use ark_scale::HOST_CALL;
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use super::*;
use gear_builtin_actor_common::bls12_381::*;
use sp_crypto_ec_utils::bls12_381;
use parity_scale_codec::{Compact, Input};

const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

pub fn handle<T: Config>(
    dispatch: &StoredDispatch,
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
where
    T::AccountId: Origin,
{
    let message = dispatch.message();
    let payload = message.payload_bytes();
    match payload.first().copied() {
        Some(REQUEST_MULTI_MILLER_LOOP) => multi_miller_loop::<T>(&payload[1..], gas_limit),
        Some(REQUEST_FINAL_EXPONENTIATION) => final_exponentiation::<T>(&payload[1..], gas_limit),
        Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1) => msm_g1::<T>(&payload[1..], gas_limit),
        Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2) => msm_g2::<T>(&payload[1..], gas_limit),
        Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1) => projective_multiplication_g1::<T>(&payload[1..], gas_limit),
        Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2) => projective_multiplication_g2::<T>(&payload[1..], gas_limit),
        _ => (0, Err(BuiltInActorReason::UnknownMessageType)),
    }
}

fn decode_vec<T: Config, I: Input>(gas_limit: u64, mut gas_spent: u64, input: &mut I) -> (u64, Option<Result<Vec<u8>, CommonError>>) {
    let Ok(len) = Compact::<u32>::decode(input).map(|l| u32::from(l)) else {
        return (gas_spent, Some(Err(CommonError::DecodeVecLength)));
    };

    let to_spend = <T as Config>::WeightInfo::decode_bytes(len).ref_time();
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, None);
    }

    gas_spent += to_spend;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();
    let result = match input.read(bytes_slice) {
        Ok(_) => Ok(items),
        Err(_) => Err(CommonError::DecodeVecData),
    };

    (gas_spent, Some(result))
}

fn multi_miller_loop<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    let (gas_spent, result) = decode_vec::<T, _>(gas_limit, 0, &mut payload);
    let a = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    let (mut gas_spent, result) = decode_vec::<T, _>(gas_limit, gas_spent, &mut payload);
    let b = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    // decode the count of items

    let mut slice = a.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED,) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode item count in a",
        );

        return (gas_spent, Ok(CommonError::DecodeItemCount.into()));
    };

    let mut slice = b.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED,) {
        Ok(count_b) if count_b != count => return (gas_spent, Ok(MultiMillerLoopResult::NonEqualItemCount.into())),
        Err(_) => {
            log::debug!(
                target: LOG_TARGET,
                "Failed to decode item count in b",
            );

            return (gas_spent, Ok(CommonError::DecodeItemCount.into()));
        }
        Ok(_) => (),
    }

    let to_spend = <T as Config>::WeightInfo::bls12_381_multi_miller_loop(count as u32).ref_time();
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, Err(BuiltInActorReason::InsufficientGas));
    }

    gas_spent += to_spend;
    let result: MultiMillerLoopResult = bls12_381::host_calls::bls12_381_multi_miller_loop(a, b).into();

    (gas_spent, Ok(result.into()))
}

fn final_exponentiation<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    let (mut gas_spent, result) = decode_vec::<T, _>(gas_limit, 0, &mut payload);
    let f = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    let to_spend = <T as Config>::WeightInfo::bls12_381_final_exponentiation().ref_time();
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, Err(BuiltInActorReason::InsufficientGas));
    }

    gas_spent += to_spend;

    (gas_spent, Ok(Response::FinalExponentiation(bls12_381::host_calls::bls12_381_final_exponentiation(f))))
}

fn msm<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Vec<u8>, ()>,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    let (gas_spent, result) = decode_vec::<T, _>(gas_limit, 0, &mut payload);
    let bases = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    let (mut gas_spent, result) = decode_vec::<T, _>(gas_limit, gas_spent, &mut payload);
    let scalars = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    // decode the count of items

    let mut slice = bases.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED,) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode item count in bases",
        );

        return (gas_spent, Ok(CommonError::DecodeItemCount.into()));
    };

    let mut slice = scalars.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED,) {
        Ok(count_b) if count_b != count => return (gas_spent, Ok(MultiScalarMultiplicationResult::NonEqualItemCount.into())),
        Err(_) => {
            log::debug!(
                target: LOG_TARGET,
                "Failed to decode item count in scalars",
            );

            return (gas_spent, Ok(CommonError::DecodeItemCount.into()));
        }
        Ok(_) => (),
    }

    let to_spend = gas_to_spend(count as u32);
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, Err(BuiltInActorReason::InsufficientGas));
    }

    gas_spent += to_spend;
    let result: MultiScalarMultiplicationResult = call(bases, scalars).into();

    (gas_spent, Ok(result.into()))
}

fn msm_g1<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    msm::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g1(count).ref_time(),
        |bases, scalars| bls12_381::host_calls::bls12_381_msm_g1(bases, scalars),
    )
}

fn msm_g2<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    msm::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_msm_g2(count).ref_time(),
        |bases, scalars| bls12_381::host_calls::bls12_381_msm_g2(bases, scalars),
    )
}

fn projective_multiplication<T: Config>(
    mut payload: &[u8],
    gas_limit: u64,
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Vec<u8>, ()>,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    let (gas_spent, result) = decode_vec::<T, _>(gas_limit, 0, &mut payload);
    let base = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    let (mut gas_spent, result) = decode_vec::<T, _>(gas_limit, gas_spent, &mut payload);
    let scalar = match result {
        Some(Ok(array)) => array,
        Some(Err(e)) => return (gas_spent, Ok(e.into())),
        None => return (gas_spent, Err(BuiltInActorReason::InsufficientGas)),
    };

    // decode the count of items

    let mut slice = scalar.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED,) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to decode item count in scalar",
        );

        return (gas_spent, Ok(CommonError::DecodeItemCount.into()));
    };

    let to_spend = gas_to_spend(count as u32);
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, Err(BuiltInActorReason::InsufficientGas));
    }

    gas_spent += to_spend;

    (gas_spent, Ok(Response::ProjectiveMultiplication(call(base, scalar))))
}

fn projective_multiplication_g1<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    projective_multiplication::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g1(count).ref_time(),
        |base, scalar| bls12_381::host_calls::bls12_381_mul_projective_g1(base, scalar),
    )
}

fn projective_multiplication_g2<T: Config>(
    payload: &[u8],
    gas_limit: u64,
) -> (u64, Result<Response, BuiltInActorReason>)
{
    projective_multiplication::<T>(
        payload,
        gas_limit,
        |count| <T as Config>::WeightInfo::bls12_381_mul_projective_g2(count).ref_time(),
        |base, scalar| bls12_381::host_calls::bls12_381_mul_projective_g2(base, scalar),
    )
}
