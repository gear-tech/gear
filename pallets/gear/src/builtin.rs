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
use gear_core::ids::{BuiltinId, ProgramId};

/// A trait representing a registry that provides methods to lookup a builtin actor
pub trait BuiltinRouter {
    type QueuedDispatch;
    type Output;

    /// Looks up a builtin actor by its actor id.
    fn lookup(&self, id: &ProgramId) -> Option<BuiltinId>;

    /// Handles a dispatch and returns an ordered sequence of outputs if the
    /// destination actor is a builtin actor, and `None` otherwise.
    fn dispatch(&self, dispatch: Self::QueuedDispatch, gas_limit: u64)
        -> Option<Vec<Self::Output>>;
}

impl BuiltinRouter for () {
    type QueuedDispatch = StoredDispatch;
    type Output = JournalNote;

    fn lookup(&self, _id: &ProgramId) -> Option<BuiltinId> {
        None
    }

    fn dispatch(
        &self,
        // _builtin_id: BuiltinId,
        _dispatch: Self::QueuedDispatch,
        _gas_limit: u64,
    ) -> Option<Vec<Self::Output>> {
        None
    }
}

pub trait BuiltinRouterProvider<Dispatch, Output> {
    type Router: BuiltinRouter<QueuedDispatch = Dispatch, Output = Output>;

    fn provide() -> Self::Router;
}

impl BuiltinRouterProvider<StoredDispatch, JournalNote> for () {
    type Router = ();

    fn provide() -> Self::Router {}
}