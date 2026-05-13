// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

use sp_core::H256;

sp_api::decl_runtime_apis! {
    pub trait GearBuiltinApi {
        /// Calculate `ActorId` (a.k.a. actor id) for a given builtin id.
        fn query_actor_id(builtin_id: u64) -> H256;
    }
}
