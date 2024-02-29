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
use ark_ec::{pairing::Pairing, Group};
use ark_std::{ops::Mul, UniformRand};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use gear_core::message::{Payload, StoredDispatch};
use parity_scale_codec::{Compact, Encode, Input};
use sp_core::MAX_POSSIBLE_ALLOCATION;

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
        let a in 0 .. (MAX_POSSIBLE_ALLOCATION - 100);

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

        let messages = {
            let mut messages = Vec::with_capacity(count);
            for _ in 0..count {
                let message: G1Affine = G1::rand(&mut rng).into();
                messages.push(message);
            }

            messages
        };

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
}

impl_benchmark_test_suite!(
    BuiltinActorPallet,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
