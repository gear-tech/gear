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
use builtins_common::bls12_381::{self, Bls12_381Ops, BlsOpsGasCost};
use core::marker::PhantomData;
use gear_runtime_interface::gear_bls_12_381 as gear_ri_bls12_381;
use sp_crypto_ec_utils::bls12_381::host_calls as sp_ri_sp_crypto_bls12_381;

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    const TYPE: BuiltinActorType = BuiltinActorType::BLS12_381;

    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        bls12_381::execute_bls12_381_builtins::<BlsOpsGasCostsImpl<T>, Bls12_381OpsRi>(
            dispatch.payload_bytes(),
            context,
        )
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
        // Returns 0 to disable pre-flight gas allowance checks.
        // Gas consumption is tracked during execution via BuiltinContext.
        // TODO: Implement payload-based gas estimation (see #4395)
        Default::default()
    }
}

struct BlsOpsGasCostsImpl<T: Config>(PhantomData<T>);

impl<T: Config> BlsOpsGasCost for BlsOpsGasCostsImpl<T> {
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

struct Bls12_381OpsRi;

impl Bls12_381Ops for Bls12_381OpsRi {
    fn multi_miller_loop(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_multi_miller_loop(g1, g2).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi Miller loop host-call failed",
            ))
        })
    }

    fn final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_final_exponentiation(f).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Final exponentiation host-call failed",
            ))
        })
    }

    fn msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_msm_g1(bases, scalars).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str("MSM G1 computation failed"))
        })
    }

    fn msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_msm_g2(bases, scalars).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str("MSM G2 computation failed"))
        })
    }

    fn projective_mul_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_mul_projective_g1(base, scalar).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Projective multiplication G1 failed",
            ))
        })
    }

    fn projective_mul_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_mul_projective_g2(base, scalar).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Projective multiplication G2 failed",
            ))
        })
    }

    fn aggregate_g1(points: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        gear_ri_bls12_381::aggregate_g1(points)
            .map_err(|err_code| BuiltinActorError::from_u32(err_code, Some("Aggregate G1 failed")))
    }

    fn map_to_g2affine(message: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        gear_ri_bls12_381::map_to_g2affine(message).map_err(|err_code| {
            BuiltinActorError::from_u32(err_code, Some("Map to G2 affine failed"))
        })
    }
}
