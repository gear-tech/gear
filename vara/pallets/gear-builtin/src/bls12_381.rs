// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use builtins_common::bls12_381::{
    self, Bls12_381Ops, BlsOpsGasCost,
    ark_bls12_381::{Bls12_381, g1::Config as G1Config, g2::Config as G2Config},
    ark_ec::{
        AffineRepr, CurveGroup,
        pairing::Pairing,
        short_weierstrass::{Affine as SWAffine, Projective as SWProjective, SWCurveConfig},
    },
    ark_scale::{
        ArkScale, ArkScaleMaxEncodedLen, HOST_CALL, MaxEncodedLen,
        hazmat::ArkScaleProjective,
        scale::{Decode, Encode},
    },
    ark_serialize::CanonicalSerialize,
};
use core::marker::PhantomData;
use gear_runtime_interface::gear_bls_12_381 as gear_ri_bls12_381;
use sp_crypto_ec_utils::bls12_381::host_calls as sp_ri_sp_crypto_bls12_381;

type ArkScaleHost<T> = ArkScale<T, HOST_CALL>;

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
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
        let mut out = buffer_for::<<Bls12_381 as Pairing>::TargetField>();
        sp_ri_sp_crypto_bls12_381::bls12_381_multi_miller_loop(&g1, &g2, &mut out).map_err(
            |_| {
                BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "Multi Miller loop host-call failed",
                ))
            },
        )?;
        Ok(out)
    }

    fn final_exponentiation(mut f: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        sp_ri_sp_crypto_bls12_381::bls12_381_final_exponentiation(&mut f).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Final exponentiation host-call failed",
            ))
        })?;
        Ok(f)
    }

    fn msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let mut out = buffer_for::<SWAffine<G1Config>>();
        sp_ri_sp_crypto_bls12_381::bls12_381_msm_g1(&bases, &scalars, &mut out).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str("MSM G1 computation failed"))
        })?;
        encode_affine_as_projective::<G1Config>(out)
    }

    fn msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let mut out = buffer_for::<SWAffine<G2Config>>();
        sp_ri_sp_crypto_bls12_381::bls12_381_msm_g2(&bases, &scalars, &mut out).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str("MSM G2 computation failed"))
        })?;
        encode_affine_as_projective::<G2Config>(out)
    }

    fn projective_mul_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let base = encode_projective_as_affine::<G1Config>(base)?;
        let mut out = buffer_for::<SWAffine<G1Config>>();
        sp_ri_sp_crypto_bls12_381::bls12_381_mul_g1(&base, &scalar, &mut out).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Projective multiplication G1 failed",
            ))
        })?;
        encode_affine_as_projective::<G1Config>(out)
    }

    fn projective_mul_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let base = encode_projective_as_affine::<G2Config>(base)?;
        let mut out = buffer_for::<SWAffine<G2Config>>();
        sp_ri_sp_crypto_bls12_381::bls12_381_mul_g2(&base, &scalar, &mut out).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Projective multiplication G2 failed",
            ))
        })?;
        encode_affine_as_projective::<G2Config>(out)
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

fn buffer_for<T>() -> Vec<u8>
where
    T: CanonicalSerialize + ArkScaleMaxEncodedLen,
{
    sp_std::vec![0; ArkScaleHost::<T>::max_encoded_len()]
}

fn encode_projective_as_affine<C>(base: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>
where
    C: SWCurveConfig,
{
    let point = ArkScaleProjective::<SWProjective<C>>::decode(&mut &base[..])
        .map_err(|_| BuiltinActorError::DecodingError)?
        .0;
    Ok(ArkScaleHost::from(point.into_affine()).encode())
}

fn encode_affine_as_projective<C>(affine: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError>
where
    C: SWCurveConfig,
{
    let affine = ArkScaleHost::<SWAffine<C>>::decode(&mut &affine[..])
        .map_err(|_| BuiltinActorError::DecodingError)?
        .0;
    Ok(ArkScaleProjective::from(affine.into_group()).encode())
}
