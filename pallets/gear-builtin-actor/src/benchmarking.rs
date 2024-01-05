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

macro_rules! impl_builtin_actor {
    ($name: ident, $id: literal) => {
        pub struct $name {}

        impl BuiltinActor<Vec<u8>, u64> for $name {
            fn handle(
                _builtin_id: BuiltinId,
                _payload: Vec<u8>,
            ) -> (Result<Vec<u8>, BuiltinActorError>, u64) {
                (Ok(Default::default()), Default::default())
            }

            fn max_gas_cost(_builtin_id: BuiltinId) -> u64 {
                Default::default()
            }
        }
        impl RegisteredBuiltinActor<Vec<u8>, u64> for $name {
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

benchmarks! {
    lookup {
        // Populate the storage with maximum (16, as of today) builtin actors ids
        for i in 0_u64..16 {
            let builtin_id = BuiltinId(i);
            let actor_id = Pallet::<T>::generate_actor_id(builtin_id);
            Actors::<T>::insert(actor_id, builtin_id);
        }
        let actor_id = ProgramId::from(100_u64);
    }: {
        BuiltinActorPallet::<T>::lookup(&actor_id)
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    calculate_id {
        let builtin_id = BuiltinId(100_u64);
    }: {
        Pallet::<T>::generate_actor_id(builtin_id)
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    base_handle_weight {
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor0, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor1, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor2, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor3, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor4, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor5, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor6, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor7, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor8, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor9, _, _>();
        let _ = BuiltinActorPallet::<T>::register_actor::<DummyActor10, _, _>();

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
    }: {
        let _ = <T as Config>::BuiltinActor::handle(builtin_id, payload);
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }
}

impl_benchmark_test_suite!(
    BuiltinActorPallet,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
