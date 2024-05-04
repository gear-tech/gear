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

//! Benchmarks for the `pallet-gear-builtin`

#[allow(unused)]
use crate::Pallet as BuiltinActorPallet;
use crate::*;
use ark_bls12_381::{Bls12_381, G1Affine, G1Projective as G1, G2Affine, G2Projective as G2};
use ark_ec::{pairing::Pairing, short_weierstrass::SWCurveConfig, Group, ScalarMul};
use ark_ff::biginteger::BigInt;
use ark_scale::hazmat::ArkScaleProjective;
use ark_std::{ops::Mul, UniformRand};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use gear_core::message::{Payload, StoredDispatch, MAX_PAYLOAD_SIZE};
use parity_scale_codec::{Compact, Encode, Input};

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;

macro_rules! impl_builtin_actor {
    ($name: ident, $id: literal) => {
        pub struct $name<T: Config>(core::marker::PhantomData<T>);

        impl<T: Config> BuiltinActor for $name<T> {
            type Error = BuiltinActorError;

            const ID: u64 = $id;

            fn handle(
                _message: &StoredDispatch,
                _gas_limit: u64,
            ) -> (Result<Payload, BuiltinActorError>, u64) {
                (Ok(Default::default()), Default::default())
            }
        }
    };
}

impl_builtin_actor!(DummyActor0, 0);
impl_builtin_actor!(DummyActor1, 1);
impl_builtin_actor!(DummyActor2, 2);
impl_builtin_actor!(DummyActor3, 3);
impl_builtin_actor!(DummyActor4, 4);
impl_builtin_actor!(DummyActor5, 5);
impl_builtin_actor!(DummyActor6, 6);
impl_builtin_actor!(DummyActor7, 7);
impl_builtin_actor!(DummyActor8, 8);
impl_builtin_actor!(DummyActor9, 9);
impl_builtin_actor!(DummyActor10, 10);
impl_builtin_actor!(DummyActor11, 11);
impl_builtin_actor!(DummyActor12, 12);
impl_builtin_actor!(DummyActor13, 13);
impl_builtin_actor!(DummyActor14, 14);
impl_builtin_actor!(DummyActor15, 15);

// This type is plugged into the Runtime when the `runtime-benchmarks` feature is enabled.
#[allow(unused)]
pub type BenchmarkingBuiltinActor<T> = (
    DummyActor0<T>,
    DummyActor1<T>,
    DummyActor2<T>,
    DummyActor3<T>,
    DummyActor4<T>,
    DummyActor5<T>,
    DummyActor6<T>,
    DummyActor7<T>,
    DummyActor8<T>,
    DummyActor9<T>,
    DummyActor10<T>,
    DummyActor11<T>,
    DummyActor12<T>,
    DummyActor13<T>,
    DummyActor14<T>,
    DummyActor15<T>,
);

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

        let mut _result: Vec<u8> = vec![];
    }: {
        _result = gear_runtime_interface::gear_bls_12_381::aggregate_g1(&encoded_points);
    } verify {
        let decoded = ArkScale::<G1>::decode(&mut &_result[..]).unwrap();
        let point_first = points.first().unwrap();
        let point_aggregated = points
            .iter()
            .skip(1)
            .fold(*point_first, |aggregated, point| aggregated + *point);

        assert_eq!(point_aggregated, decoded.0);
    }

    map_to_g2 {
        let c in 1 .. 1_000;

        let count = c as usize;

        let message = vec![1u8; count];

        let mut _result: Result<Vec<u8>, u32> = Err(0);
    }: {
        _result = gear_runtime_interface::gear_bls_12_381::map_to_g2(&message);
    } verify {
        assert!(_result.is_ok());
        let decoded = ArkScale::<G2Affine>::decode(&mut &_result.unwrap()[..]).unwrap();

        use ark_bls12_381::{G2Projective as G2, G2Affine};
        use ark_ff::fields::field_hashers::DefaultFieldHasher;
        use ark_ec::{
            hashing::{map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve, curve_maps::wb},
            bls12::Bls12Config,
        };

        type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

        // Domain Separation Tag for signatures on G2
        pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
        let mapper = MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2).unwrap();
        let point = mapper.hash(&message).unwrap();

        assert_eq!(point, decoded.0);
    }
}

impl_benchmark_test_suite!(
    BuiltinActorPallet,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
