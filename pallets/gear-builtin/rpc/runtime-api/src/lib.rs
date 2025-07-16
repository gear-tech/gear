// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use pallet_gear_builtin::BuiltinActorType;
use sp_core::H256;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
    pub trait GearBuiltinApi {
        /// Calculate `ActorId` (a.k.a. actor id) for a given builtin id.
        fn query_actor_id(builtin_id: u64) -> H256;
        /// Get list of all current builtin actors.
        fn list_actors() -> Vec<(BuiltinActorType, u16, H256)>;
        /// Get specific builtin `ActorId` by its type.
        fn get_actor_id(actor_type: BuiltinActorType, version: u16) -> Option<H256>;
    }
}
