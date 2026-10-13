// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Benchmarks for Pallet Gear Eth Bridge.

use crate::{Call, Config, CurrencyOf, Pallet};
use common::{Origin, benchmarking};
use frame_benchmarking::benchmarks;
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use gprimitives::{H160, H256};
use parity_scale_codec::Decode;
use sp_runtime::traits::{Get, UniqueSaturatedInto};
use sp_std::vec;

#[cfg(test)]
use crate::mock;

benchmarks! {
    where_clause { where T::AccountId: Origin }

    pause {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);
        // Generic benchmarks use the legacy lane, not runtime BEEFY readiness fixtures.
        crate::DestinationBinding::<T>::kill();
        crate::BridgeDomain::<T>::kill();

        // Initially pallet is paused so we need to unpause it first.
        assert!(Pallet::<T>::unpause(RawOrigin::Root.into()).is_ok());
    }: _(RawOrigin::Root)
    verify {
        assert!(crate::Paused::<T>::get());
    }

    unpause {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);
        crate::DestinationBinding::<T>::kill();
        crate::BridgeDomain::<T>::kill();
    }: _(RawOrigin::Root)
    verify {
        assert!(!crate::Paused::<T>::get());
    }

    set_fee {
        let fee = 4242424242424242u128.unique_saturated_into();
    } : _(RawOrigin::Root, fee)
    verify {
        assert_eq!(crate::TransportFee::<T>::get(), 4242424242424242u128.unique_saturated_into());
    }

    bind_destination {
        let genesis = T::Hash::decode(&mut &[9u8; 32][..]).expect("32-byte runtime hash");
        frame_system::BlockHash::<T>::insert(frame_system::pallet_prelude::BlockNumberFor::<T>::from(0u32), genesis);
        crate::DestinationBinding::<T>::kill();
        crate::BridgeDomain::<T>::kill();
        crate::Paused::<T>::put(true);
        let chain_id = H256::repeat_byte(1);
        let queue = H160::repeat_byte(3);
    }: _(RawOrigin::Root, chain_id, queue)
    verify {
        let source = H256::from_slice(genesis.as_ref());
        assert_eq!(crate::DestinationBinding::<T>::get(), Some((source, chain_id, queue)));
        assert_eq!(Pallet::<T>::bridge_domain(), Pallet::<T>::destination_domain(source, chain_id, queue));
    }

    send_eth_message {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);
        crate::DestinationBinding::<T>::kill();
        crate::BridgeDomain::<T>::kill();

        // Set fee to minimum balance for the benchmark.
        assert!(Pallet::<T>::set_fee(RawOrigin::Root.into(), CurrencyOf::<T>::minimum_balance()).is_ok());
        // Initially pallet is paused so we need to unpause it first.
        assert!(Pallet::<T>::unpause(RawOrigin::Root.into()).is_ok());

        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        let _ = crate::CurrencyOf::<T>::deposit_creating(&origin, CurrencyOf::<T>::minimum_balance());

        let destination = [42; 20].into();

        let payload = vec![42; T::MaxPayloadSize::get() as usize];
        // Decode and rewrite the full ordinary queue on its last free slot.
        crate::Queue::<T>::put(vec![H256::repeat_byte(7); T::QueueCapacity::get().saturating_sub(1) as usize]);
    }: _(RawOrigin::Signed(origin), destination, payload)
    verify {
        assert_eq!(crate::Queue::<T>::get().len(), T::QueueCapacity::get() as usize);
    }

    finalize_full_queue {
        let queue = (0..T::QueueCapacity::get()).map(|i| {
            let mut hash = [0; 32];
            hash[28..].copy_from_slice(&i.to_be_bytes());
            H256::from(hash)
        }).collect::<sp_std::vec::Vec<_>>();
        let expected = binary_merkle_tree::merkle_root_raw::<sp_runtime::traits::Keccak256, _>(queue.iter().copied());
        crate::Queue::<T>::put(queue);
        crate::QueueChanged::<T>::put(true);
    }: {
        Pallet::<T>::update_queue_merkle_root_if_changed();
    } verify {
        assert_eq!(crate::QueueMerkleRoot::<T>::get(), Some(expected));
        assert!(!crate::QueueChanged::<T>::get());
        assert_eq!(crate::QueueOverflowedSince::<T>::get(), Some(frame_system::Pallet::<T>::block_number()));
    }

    impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
