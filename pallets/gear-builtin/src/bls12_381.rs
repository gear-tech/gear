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

use builtins_common::bls12_381::*;
use core::marker::PhantomData;
use gbuiltin_bls381::*;
use gear_runtime_interface::gear_bls_12_381 as ri_gear_bls_12_381;

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

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        // todo [sab] this body can also be moved to a separate function and re-used
        let payload = dispatch.payload_bytes();
        let res = match payload.first().copied() {
            Some(REQUEST_MULTI_MILLER_LOOP) => {
                multi_miller_loop::<BlsOpsGasCostsImpl<T>>(&payload[1..], context, |a, b| {
                    ri_gear_bls_12_381::multi_miller_loop(a, b)
                        .map_err(|err_code| BuiltinActorError::from_u32(err_code, None))
                })
                .map(Response::MultiMillerLoop)
            }
            Some(REQUEST_FINAL_EXPONENTIATION) => {
                final_exponentiation::<BlsOpsGasCostsImpl<T>>(&payload[1..], context, |f| {
                    ri_gear_bls_12_381::final_exponentiation(f).map_err(|err_code| {
                        BuiltinActorError::from_u32(err_code, Some("Final exponentiation failed"))
                    })
                })
                .map(Response::FinalExponentiation)
            }
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G1) => msm::<BlsOpsGasCostsImpl<T>>(
                &payload[1..],
                context,
                |count| T::WeightInfo::bls12_381_msm_g1(count).ref_time(),
                |bases, scalars| {
                    ri_gear_bls_12_381::msm_g1(bases, scalars).map_err(|err_code| {
                        BuiltinActorError::from_u32(err_code, Some("MSM G1 computation failed"))
                    })
                },
            )
            .map(Response::MultiScalarMultiplicationG1),
            Some(REQUEST_MULTI_SCALAR_MULTIPLICATION_G2) => msm::<BlsOpsGasCostsImpl<T>>(
                &payload[1..],
                context,
                |count| T::WeightInfo::bls12_381_msm_g2(count).ref_time(),
                |bases, scalars| {
                    ri_gear_bls_12_381::msm_g2(bases, scalars).map_err(|err_code| {
                        BuiltinActorError::from_u32(err_code, Some("MSM G2 computation failed"))
                    })
                },
            )
            .map(Response::MultiScalarMultiplicationG2),
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G1) => {
                projective_multiplication::<BlsOpsGasCostsImpl<T>>(
                    &payload[1..],
                    context,
                    |count| T::WeightInfo::bls12_381_mul_projective_g1(count).ref_time(),
                    |base, scalar| {
                        ri_gear_bls_12_381::projective_mul_g1(base, scalar)
                            .map_err(|err_code| BuiltinActorError::from_u32(err_code, None))
                    },
                )
                .map(Response::ProjectiveMultiplicationG1)
            }
            Some(REQUEST_PROJECTIVE_MULTIPLICATION_G2) => {
                projective_multiplication::<BlsOpsGasCostsImpl<T>>(
                    &payload[1..],
                    context,
                    |count| T::WeightInfo::bls12_381_mul_projective_g2(count).ref_time(),
                    |base, scalar| {
                        ri_gear_bls_12_381::projective_mul_g2(base, scalar)
                            .map_err(|err_code| BuiltinActorError::from_u32(err_code, None))
                    },
                )
                .map(Response::ProjectiveMultiplicationG2)
            }
            Some(REQUEST_AGGREGATE_G1) => {
                aggregate_g1::<BlsOpsGasCostsImpl<T>>(&payload[1..], context, |points| {
                    ri_gear_bls_12_381::aggregate_g1(points)
                        .map_err(|err_code| BuiltinActorError::from_u32(err_code, None))
                })
                .map(Response::AggregateG1)
            }
            Some(REQUEST_MAP_TO_G2AFFINE) => {
                map_to_g2affine::<BlsOpsGasCostsImpl<T>>(&payload[1..], context, |message| {
                    ri_gear_bls_12_381::map_to_g2affine(message)
                        .map_err(|err_code| BuiltinActorError::from_u32(err_code, None))
                })
                .map(Response::MapToG2Affine)
            }
            _ => Err(BuiltinActorError::DecodingError),
        };

        res.map(|response| BuiltinReply {
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
