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

benchmarks! {
    calculate_id {
        let builtin_id = BuiltinId(100_u64);
    }: {
        Pallet::<T>::generate_actor_id(builtin_id)
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }

    base_handle_weight {
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
        let builtin_message = SimpleBuiltinMessage {
            source,
            destination: builtin_id,
            payload,
        };
    }: {
        let _ = <T as Config>::BuiltinActor::handle(&builtin_message, 1_000_000);
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }
}

impl_benchmark_test_suite!(
    BuiltinActorPallet,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
