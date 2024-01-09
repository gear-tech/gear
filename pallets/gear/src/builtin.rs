// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;
use gear_core::ids::BuiltinId;

/// A trait representing a registry that provides methods to lookup a builtin actor
pub trait BuiltinRouter<ActorId> {
    type Dispatch;
    type Output;

    /// Returns available actors identifiers.
    fn lookup(id: &ActorId) -> Option<BuiltinId>;

    /// Handles a builtin actor dispatch and returns an order sequence of outputs.
    fn dispatch(
        builtin_id: BuiltinId,
        dispatch: Self::Dispatch,
        gas_limit: u64,
    ) -> Vec<Self::Output>;

    /// Upper bound for gas required to handle a message by a builtin actor.
    fn estimate_gas(builtin_id: BuiltinId) -> u64;
}

impl<ActorId> BuiltinRouter<ActorId> for () {
    type Dispatch = StoredDispatch;
    type Output = JournalNote;

    fn lookup(_id: &ActorId) -> Option<BuiltinId> {
        None
    }

    fn dispatch(
        _builtin_id: BuiltinId,
        _dispatch: Self::Dispatch,
        _gas_limit: u64,
    ) -> Vec<Self::Output> {
        Default::default()
    }

    fn estimate_gas(_builtin_id: BuiltinId) -> u64 {
        Default::default()
    }
}
