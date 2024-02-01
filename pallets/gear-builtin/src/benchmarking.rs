// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Benchmarks for the gear-built-in-actor pallet

#[allow(unused)]
use crate::Pallet as BuiltinActorPallet;
use crate::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use gear_core::{
    ids::{BuiltinId, ProgramId},
    message::{DispatchKind, StoredDispatch, StoredMessage},
};

type BuiltinMessageFor<T> = WithBytesPayload<T>;

macro_rules! impl_builtin_actor {
    ($name: ident, $id: literal) => {
        pub struct $name<T: Config>(core::marker::PhantomData<T>);

        impl<T: Config> BuiltinActor<BuiltinMessageFor<T>, u64> for $name<T> {
            fn handle(
                _message: &BuiltinMessageFor<T>,
                _gas_limit: u64,
            ) -> (Result<Vec<u8>, BuiltinActorError>, u64) {
                (Ok(Default::default()), Default::default())
            }

            fn get_ids(buffer: &mut Vec<BuiltinId>) {
                buffer.push(Self::ID);
            }
        }
        impl<T: Config> RegisteredBuiltinActor<BuiltinMessageFor<T>, u64> for $name<T> {
            const ID: BuiltinId = BuiltinId($id as u64);
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
        let builtin_id = BuiltinId(100_u64);
    }: {
        Pallet::<T>::generate_actor_id(builtin_id)
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    provide {
        let builtin_id = BuiltinId(10_u64);
        let actor_id = BuiltinActorPallet::<T>::generate_actor_id(builtin_id);
        let payload = b"Payload".to_vec();
        let source = ProgramId::from(255_u64);

        let dispatch = StoredDispatch::new(
            DispatchKind::Handle,
            StoredMessage::new(
                Default::default(),
                source,
                actor_id,
                payload.clone().try_into().unwrap(),
                0_u128,
                None,
            ),
            None,
        );
        let gas_limit = 10_000_000_000_u64;
        let builtin_message = BuiltinMessageFor::<T> {
            source,
            destination: builtin_id,
            payload,
            _phantom: Default::default(),
        };
    }: {
        let _ = <T as pallet_gear::Config>::BuiltinRouter::provide();
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }
}

impl_benchmark_test_suite!(
    BuiltinActorPallet,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
