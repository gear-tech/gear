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
use crate::Pallet as BuiltInActor;
use crate::*;
use common::{benchmarking, Origin};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use gear_builtin_actor_common::staking::StakingMessage;
use gear_core::message::{DispatchKind, StoredDispatch, StoredMessage};
use pallet_gear::BuiltInActor as BuiltInActorT;
use parity_scale_codec::Encode;
use sp_runtime::traits::UniqueSaturatedInto;

pub(crate) type CurrencyOf<T> = <T as pallet_staking::Config>::Currency;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    base_handle_weight {
        let issuer = benchmarking::account::<T::AccountId>("caller", 0, 0);
        CurrencyOf::<T>::deposit_creating(
            &issuer,
            1_000_000_000_000_000_u128.unique_saturated_into()
        );
        let built_in_actor_id = BuiltInActor::<T>::staking_proxy_actor_id();
        let value = 100_000_000_000_000_u128;
        let payload = StakingMessage::Bond { value }.encode();
        let source = ProgramId::from_origin(issuer.clone().into_origin());

        let dispatch = StoredDispatch::new(
            DispatchKind::Handle,
            StoredMessage::new(
                Default::default(),
                source,
                built_in_actor_id,
                payload.try_into().unwrap(),
                value,
                None,
            ),
            None,
        );
        let gas_limit = 10_000_000_000_u64;
    }: {
        BuiltInActor::<T>::handle(
            dispatch,
            gas_limit,
        )
    } verify {
        // No changes in runtime are expected since the actual dispatch doesn't take place.
    }
}

impl_benchmark_test_suite!(BuiltInActor, crate::mock::new_test_ext(), crate::mock::Test,);
