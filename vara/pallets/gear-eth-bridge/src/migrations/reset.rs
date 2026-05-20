// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This migration totally resets bridge.

use crate::{
    Config, MessageNonce, Queue, QueueChanged, QueueId, QueueMerkleRoot, QueueOverflowedSince,
    QueuesInfo,
};
use frame_support::{
    pallet_prelude::Weight,
    traits::{Get, OnRuntimeUpgrade},
};
use gprimitives::H256;
#[cfg(feature = "try-runtime")]
use {sp_runtime::TryRuntimeError, sp_std::vec::Vec};

/// Migration that totally resets bridge.
pub struct ResetMigration<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for ResetMigration<T> {
    // Uncomment code below for full wipe: including initialization.
    fn on_runtime_upgrade() -> Weight {
        let mut weight = Weight::zero();
        let db_weight = T::DbWeight::get();

        // Initialized::<T>::kill();
        // weight = weight.saturating_add(db_weight.writes(1));

        // Paused::<T>::kill();
        // weight = weight.saturating_add(db_weight.writes(1));

        // AuthoritySetHash::<T>::kill();
        // weight = weight.saturating_add(db_weight.writes(1));

        QueueMerkleRoot::<T>::put(H256::zero());
        weight = weight.saturating_add(db_weight.writes(1));

        Queue::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        QueueId::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        let res = QueuesInfo::<T>::clear(u32::MAX, None);
        weight = weight.saturating_add(db_weight.writes(res.unique.into()));

        // ClearTimer::<T>::kill();
        // weight = weight.saturating_add(db_weight.writes(1));

        MessageNonce::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        QueueChanged::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        QueueOverflowedSince::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        // TransportFee::<T>::kill();
        // weight = weight.saturating_add(db_weight.writes(1));

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok(Default::default())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
        Ok(())
    }
}
