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
use ark_bls12_381::{
    Bls12_381, G1Projective, G2Projective, g1::Config as G1Config, g2::Config as G2Config,
};
use ark_ec::{
    CurveConfig, VariableBaseMSM,
    bls12::Bls12Config,
    hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::{MillerLoopOutput, Pairing},
    short_weierstrass::{Affine as SWAffine, Projective as SWProjective, SWCurveConfig},
};
use ark_ff::fields::field_hashers::DefaultFieldHasher;
use ark_scale::{
    HOST_CALL,
    scale::{Decode, Encode},
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use gear_core::limited::LimitedStr;
use sha2;

type ArkScaleLocal<T> = ark_scale::ArkScale<T, HOST_CALL>;
const _: () = assert!(HOST_CALL == ark_scale::make_usage(Compress::No, Validate::No));
type ArkScaleProjective<T> = ark_scale::hazmat::ArkScaleProjective<T>;

/// Implementation of the bls12-381 operations, intended to be used
/// directly.
///
/// Current impl is also used as a base impl for host-call based impl.
/// The methods impl are considered to be low-level. To actually execute
/// bls12-381 operations separate functions defined in the `builtins_common::bls12_381`
/// must be used.
pub struct Bls12_381OpsLowLevel;

/// Basically, copies impl of bls12_381 operations from `sp-crypto-ec-utils` crate
impl Bls12_381Ops for Bls12_381OpsLowLevel {
    fn multi_miller_loop(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let a = Self::decode::<Vec<<Bls12_381 as Pairing>::G1Affine>>(g1)?;
        let b = Self::decode::<Vec<<Bls12_381 as Pairing>::G2Affine>>(g2)?;
        let res = Bls12_381::multi_miller_loop(a, b);

        Ok(Self::encode(res.0))
    }

    fn final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let f = Self::decode::<<Bls12_381 as Pairing>::TargetField>(f)?;
        let res = Bls12_381::final_exponentiation(MillerLoopOutput(f)).ok_or(
            BuiltinActorError::Custom(LimitedStr::from_small_str("Final exponentiation failed")),
        )?;

        Ok(Self::encode(res.0))
    }

    fn msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        Self::msm_sw::<G1Config>(bases, scalars)
    }

    fn msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        Self::msm_sw::<G2Config>(bases, scalars)
    }

    fn projective_mul_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        Self::projective_mul_sw::<G1Config>(base, scalar)
    }

    fn projective_mul_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        Self::projective_mul_sw::<G2Config>(base, scalar)
    }

    fn aggregate_g1(points: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        let points = Self::decode::<Vec<G1Projective>>(points)?;

        let point_first = points.first().ok_or(BuiltinActorError::EmptyG1PointsList)?;

        let point_aggregated = points
            .iter()
            .skip(1)
            .fold(*point_first, |aggregated, point| aggregated + *point);

        Ok(Self::encode(point_aggregated))
    }

    fn map_to_g2affine(message: Vec<u8>) -> Result<Vec<u8>, BuiltinActorError> {
        type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

        // Domain Separation Tag for signatures on G2.
        const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

        let mapper =
            MapToCurveBasedHasher::<G2Projective, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(
                DST_G2,
            )
            .map_err(|_| BuiltinActorError::MapperCreationError)?;

        let point = mapper
            .hash(&message)
            .map_err(|_| BuiltinActorError::MessageMappingError)?;

        Ok(Self::encode(point))
    }
}

impl Bls12_381OpsLowLevel {
    fn msm_sw<T: SWCurveConfig>(
        bases: Vec<u8>,
        scalars: Vec<u8>,
    ) -> Result<Vec<u8>, BuiltinActorError> {
        let bases = Self::decode::<Vec<SWAffine<T>>>(bases)?;
        let scalars = Self::decode::<Vec<<T as CurveConfig>::ScalarField>>(scalars)?;
        let res = <SWProjective<T> as VariableBaseMSM>::msm(&bases, &scalars).map_err(|_| {
            BuiltinActorError::Custom(LimitedStr::from_small_str("MSM SW: computation error"))
        })?;

        Ok(Self::encode_proj_sw(&res))
    }

    fn projective_mul_sw<T: SWCurveConfig>(
        base: Vec<u8>,
        scalar: Vec<u8>,
    ) -> Result<Vec<u8>, BuiltinActorError> {
        let base = Self::decode_proj_sw::<T>(base)?;
        let scalar = Self::decode::<Vec<u64>>(scalar)?;
        let res = <T as SWCurveConfig>::mul_projective(&base, &scalar);

        Ok(Self::encode_proj_sw(&res))
    }

    fn encode<T: CanonicalSerialize>(val: T) -> Vec<u8> {
        ArkScaleLocal::from(val).encode()
    }

    fn decode<T: CanonicalDeserialize>(buf: Vec<u8>) -> Result<T, BuiltinActorError> {
        ArkScaleLocal::<T>::decode(&mut &buf[..])
            .map(|v| v.0)
            .map_err(|_| BuiltinActorError::DecodingError)
    }

    fn encode_proj_sw<T: SWCurveConfig>(val: &SWProjective<T>) -> Vec<u8> {
        ArkScaleProjective::from(val).encode()
    }

    fn decode_proj_sw<T: SWCurveConfig>(
        buf: Vec<u8>,
    ) -> Result<SWProjective<T>, BuiltinActorError> {
        ArkScaleProjective::decode(&mut &buf[..])
            .map(|v| v.0)
            .map_err(|_| BuiltinActorError::DecodingError)
    }
}
