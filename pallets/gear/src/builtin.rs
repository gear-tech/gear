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

use super::*;
use core_processor::common::JournalNote;
use gear_core::{ids::ActorId, message::StoredDispatch};

/// The result of a builtin actor `handle` call.
#[derive(Debug)]
pub struct HandleFnResult {
    pub payload: Payload,
    pub return_value: Value,
}

/// Builtin actor `handle` function signature.
pub type HandleFn<C, E> = dyn Fn(&StoredDispatch, &mut C) -> Result<HandleFnResult, E>;

/// Builtin actor `max_gas` function signature.
// TODO: #4395. Let the weight function take complexity arguments for more accurate gas estimation.
pub type WeightFn = dyn Fn() -> u64;

pub struct BuiltinInfo<'a, C, E> {
    pub handle: &'a HandleFn<C, E>,
    pub max_gas: &'a WeightFn,
}

/// A trait representing a registry that provides methods to lookup and run a builtin actor.
pub trait BuiltinDispatcher {
    type Context;
    type Error;

    /// Looks up a builtin actor by its actor id.
    fn lookup(&self, id: &ActorId) -> Option<BuiltinInfo<Self::Context, Self::Error>>;

    fn run(
        &self,
        context: BuiltinInfo<Self::Context, Self::Error>,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<JournalNote>;
}

impl BuiltinDispatcher for () {
    type Context = ();
    type Error = ();

    fn lookup(&self, _id: &ActorId) -> Option<BuiltinInfo<Self::Context, Self::Error>> {
        None
    }

    fn run(
        &self,
        _context: BuiltinInfo<Self::Context, Self::Error>,
        _dispatch: StoredDispatch,
        _gas_limit: u64,
    ) -> Vec<JournalNote> {
        Default::default()
    }
}

/// A trait that defines the interface of a builtin dispatcher factory.
pub trait BuiltinDispatcherFactory {
    type Context;
    type Error;
    type Output: BuiltinDispatcher<Context = Self::Context, Error = Self::Error>;

    fn create() -> (Self::Output, u64);
}

impl BuiltinDispatcherFactory for () {
    type Context = ();
    type Error = ();
    type Output = ();

    fn create() -> (Self::Output, u64) {
        ((), 0)
    }
}
