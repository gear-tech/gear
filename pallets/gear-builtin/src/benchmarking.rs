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

//! Benchmarks for the `pallet-gear-builtin`

#[allow(unused)]
use crate::Pallet as BuiltinActorPallet;
use crate::*;
use ark_std::{UniformRand, ops::Mul};
use builtins_common::bls12_381::{
    ark_bls12_381::{self, Bls12_381, G1Affine, G1Projective as G1, G2Affine, G2Projective as G2},
    ark_ec::{Group, ScalarMul, pairing::Pairing, short_weierstrass::SWCurveConfig},
    ark_ff::biginteger::BigInt,
    ark_scale::{self, hazmat::ArkScaleProjective},
};
use common::Origin;
use frame_benchmarking::benchmarks;
use gear_core::buffer::MAX_PAYLOAD_SIZE;
use parity_scale_codec::{Compact, Encode, Input};

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;

const MAX_BIG_INT: u32 = 100;

fn naive_var_base_msm<G: ScalarMul>(bases: &[G::MulBase], scalars: &[G::ScalarField]) -> G {
    let mut acc = G::zero();

    for (base, scalar) in bases.iter().zip(scalars.iter()) {
        acc += *base * scalar;
    }

    acc
}

benchmarks! {
    where_clause {
        where
            T: pallet_gear::Config,
            T::AccountId: Origin,
    }

    calculate_id {
        let builtin_id = 100_u64;
    }: {
        Pallet::<T>::generate_actor_id(builtin_id)
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    create_dispatcher {
    }: {
        let _ = <T as pallet_gear::Config>::BuiltinDispatcherFactory::create();
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    decode_bytes {
        let a in 1 .. MAX_PAYLOAD_SIZE as u32;

        let bytes = vec![1u8; a as usize];
        let encoded = bytes.encode();
        let mut _decoded = vec![];
    }: {
        let mut input = encoded.as_slice();
        let len = u32::from(Compact::<u32>::decode(&mut input).unwrap()) as usize;

        let mut items = vec![0u8; len];
        let bytes_slice = items.as_mut_slice();
        input.read(bytes_slice).unwrap();

        _decoded = items;
    } verify {
        assert_eq!(bytes, _decoded);
    }

    bls12_381_multi_miller_loop {
        let c in 0 .. 100;

        let count = c as usize;

        let mut rng = ark_std::test_rng();

        let messages = (0..count).map(|_| G1::rand(&mut rng).into()).collect::<Vec<G1Affine>>();

        let message: ArkScale<Vec<<Bls12_381 as Pairing>::G1Affine>> = messages.into();
        let encoded_message = message.encode();

        let pub_keys = {
            let mut pub_keys = Vec::with_capacity(count);
            let generator: G2 = G2::generator();
            for _ in 0..count {
                let priv_key: ScalarField = UniformRand::rand(&mut rng);
                let pub_key: G2Affine = generator.mul(priv_key).into();
                pub_keys.push(pub_key);
            }

            pub_keys
        };
        let pub_key: ArkScale<Vec<<Bls12_381 as Pairing>::G2Affine>> = pub_keys.into();
        let encoded_pub_key = pub_key.encode();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_multi_miller_loop(
            encoded_message,
            encoded_pub_key,
        );
    } verify {
        assert!(_result.is_ok());
    }

    bls12_381_final_exponentiation {
        let mut rng = ark_std::test_rng();

        // message
        let message: G1Affine = G1::rand(&mut rng).into();
        let message: ArkScale<Vec<<Bls12_381 as Pairing>::G1Affine>> = vec![message].into();
        let encoded_message = message.encode();

        let priv_key: ScalarField = UniformRand::rand(&mut rng);
        let generator: G2 = G2::generator();
        let pub_key: G2Affine = generator.mul(priv_key).into();
        let pub_key: ArkScale<Vec<<Bls12_381 as Pairing>::G2Affine>> = vec![pub_key].into();
        let encoded_pub_key = pub_key.encode();

        let miller_loop = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_multi_miller_loop(
            encoded_message,
            encoded_pub_key,
        ).unwrap();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_final_exponentiation(miller_loop);
    } verify {
        assert!(_result.is_ok());
    }

    bls12_381_msm_g1 {
        let c in 1 .. 1_000;

        let count = c as usize;

        let mut rng = ark_std::test_rng();

        let scalar = (0..count)
            .map(|_| <G1 as Group>::ScalarField::rand(&mut rng))
            .max()
            .unwrap();
        let scalars = vec![scalar; count];
        let ark_scalars: ArkScale<Vec<<G1 as Group>::ScalarField>> = scalars.clone().into();
        let encoded_scalars = ark_scalars.encode();

        let bases = (0..count).map(|_| G1::rand(&mut rng)).collect::<Vec<_>>();
        let bases = G1::batch_convert_to_mul_base(&bases);
        let ark_bases: ArkScale<Vec<G1Affine>> = bases.clone().into();
        let encoded_bases = ark_bases.encode();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_msm_g1(encoded_bases, encoded_scalars);
    } verify {
        let naive = naive_var_base_msm::<G1>(bases.as_slice(), scalars.as_slice());
        let encoded = _result.unwrap();
        let fast = ArkScaleProjective::<G1>::decode(&mut &encoded[..]).unwrap();
        assert_eq!(naive, fast.0);
    }

    bls12_381_msm_g2 {
        let c in 1 .. 1_000;

        let count = c as usize;

        let mut rng = ark_std::test_rng();

        let scalar = (0..count)
            .map(|_| <G2 as Group>::ScalarField::rand(&mut rng))
            .max()
            .unwrap();
        let scalars = vec![scalar; count];
        let ark_scalars: ArkScale<Vec<<G2 as Group>::ScalarField>> = scalars.clone().into();
        let encoded_scalars = ark_scalars.encode();

        let bases = (0..count).map(|_| G2::rand(&mut rng)).collect::<Vec<_>>();
        let bases = G2::batch_convert_to_mul_base(&bases);
        let ark_bases: ArkScale<Vec<G2Affine>> = bases.clone().into();
        let encoded_bases = ark_bases.encode();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_msm_g2(encoded_bases, encoded_scalars);
    } verify {
        let naive = naive_var_base_msm::<G2>(bases.as_slice(), scalars.as_slice());
        let encoded = _result.unwrap();
        let fast = ArkScaleProjective::<G2>::decode(&mut &encoded[..]).unwrap();
        assert_eq!(naive, fast.0);
    }

    bls12_381_mul_projective_g1 {
        let c in 1 .. MAX_BIG_INT;

        let mut rng = ark_std::test_rng();

        let bigint = BigInt::<{ MAX_BIG_INT as usize }>::rand(&mut rng);
        let bigint = bigint.as_ref()[..c as usize].to_vec();
        let ark_bigint: ArkScale<Vec<u64>> = bigint.clone().into();
        let encoded_bigint = ark_bigint.encode();

        let base = G1::rand(&mut rng);
        let ark_base: ArkScaleProjective<G1> = base.into();
        let encoded_base = ark_base.encode();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_mul_projective_g1(encoded_base, encoded_bigint);
    } verify {
        let encoded = _result.unwrap();
        let result = ArkScaleProjective::<G1>::decode(&mut &encoded[..]).unwrap();
        let standard = <ark_bls12_381::g1::Config as SWCurveConfig>::mul_projective(&base, &bigint);
        assert_eq!(standard, result.0);
    }

    bls12_381_mul_projective_g2 {
        let c in 1 .. MAX_BIG_INT;

        let mut rng = ark_std::test_rng();

        let bigint = BigInt::<{ MAX_BIG_INT as usize }>::rand(&mut rng);
        let bigint = bigint.as_ref()[..c as usize].to_vec();
        let ark_bigint: ArkScale<Vec<u64>> = bigint.clone().into();
        let encoded_bigint = ark_bigint.encode();

        let base = G2::rand(&mut rng);
        let ark_base: ArkScaleProjective<G2> = base.into();
        let encoded_base = ark_base.encode();

        let mut _result: Result<Vec<u8>, ()> = Err(());
    }: {
        _result = sp_crypto_ec_utils::bls12_381::host_calls::bls12_381_mul_projective_g2(encoded_base, encoded_bigint);
    } verify {
        let encoded = _result.unwrap();
        let result = ArkScaleProjective::<G2>::decode(&mut &encoded[..]).unwrap();
        let standard = <ark_bls12_381::g2::Config as SWCurveConfig>::mul_projective(&base, &bigint);
        assert_eq!(standard, result.0);
    }

    bls12_381_aggregate_g1 {
        let c in 1 .. 1_000;

        let count = c as usize;

        let mut rng = ark_std::test_rng();

        let points = (0..count).map(|_| G1::rand(&mut rng)).collect::<Vec<_>>();
        let ark_points: ArkScale<Vec<G1>> = points.clone().into();
        let encoded_points = ark_points.encode();

        // Custom error by default.
        let mut _result = Err(3);
    }: {
        _result = gear_runtime_interface::gear_bls_12_381::aggregate_g1(encoded_points);
    } verify {
        assert!(
            matches!(_result, Ok(result) if ArkScale::<G1>::decode(&mut &result[..]).is_ok())
        );
    }

    bls12_381_map_to_g2affine {
        let c in 0 .. MAX_PAYLOAD_SIZE as u32;

        let message = vec![1u8; c as usize];

        // Custom error by default.
        let mut _result = Err(3);
    }: {
        _result = gear_runtime_interface::gear_bls_12_381::map_to_g2affine(message);
    } verify {
        assert!(ArkScale::<G2Affine>::decode(&mut &_result.unwrap()[..]).is_ok())
    }

    impl_benchmark_test_suite!(
        BuiltinActorPallet,
        crate::mock::new_test_ext(),
        crate::mock::Test,
    );
}
