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
use impl_trait_for_tuples::impl_for_tuples;
use sp_runtime::DispatchError;

/// A trait representing a registry that provides methods to lookup a builtin actor
pub trait BuiltinLookup<ActorId> {
    /// Returns available actors identifiers.
    fn lookup(id: &ActorId) -> Option<BuiltinId>;
}

impl<ActorId> BuiltinLookup<ActorId> for () {
    fn lookup(_id: &ActorId) -> Option<BuiltinId> {
        None
    }
}

/// A trait representing an interface of a builtin actor that can receive a message
/// and produce a set of outputs that can then be converted into a reply message.
pub trait BuiltinActor<Dispatch, Output> {
    /// Handles a message and returns an ordered sequence of outputs.
    fn handle(
        builtin_id: BuiltinId,
        dispatch: Dispatch,
        gas_limit: u64,
    ) -> Result<Vec<Output>, DispatchError>;
}

pub trait RegisteredBuiltinActor<Dispatch, Output>: BuiltinActor<Dispatch, Output> {
    /// The global unique ID of the trait implementer type
    const ID: BuiltinId;
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(RegisteredBuiltinActor<D, O>)]
impl<D, O> BuiltinActor<D, O> for Tuple {
    fn handle(builtin_id: BuiltinId, dispatch: D, gas_limit: u64) -> Result<Vec<O>, DispatchError> {
        for_tuples!(
            #(
                if (Tuple::ID == builtin_id) {
                    return Tuple::handle(builtin_id, dispatch, gas_limit);
                }
            )*
        );
        Err(DispatchError::Unavailable)
    }
}
