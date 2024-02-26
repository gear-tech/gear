// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use gear_core::{
    ids::ProgramId,
    message::{Payload, StoredDispatch},
};

pub type HandleFn<E> = dyn Fn(&StoredDispatch, u64) -> (Result<Payload, E>, u64);

/// A trait representing a registry that provides methods to lookup and run a builtin actor.
pub trait BuiltinDispatcher {
    type Error;

    /// Looks up a builtin actor by its actor id.
    fn lookup<'a>(&'a self, id: &ProgramId) -> Option<&'a HandleFn<Self::Error>>;

    fn run(
        &self,
        f: &HandleFn<Self::Error>,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<JournalNote>;
}

impl BuiltinDispatcher for () {
    type Error = ();

    fn lookup<'a>(&'a self, _id: &ProgramId) -> Option<&'a HandleFn<Self::Error>> {
        None
    }

    fn run(
        &self,
        _f: &HandleFn<Self::Error>,
        _dispatch: StoredDispatch,
        _gas_limit: u64,
    ) -> Vec<JournalNote> {
        Default::default()
    }
}

/// A trait that defines the interface of a builtin dispatcher factory.
pub trait BuiltinDispatcherFactory {
    type Error;
    type Output: BuiltinDispatcher<Error = Self::Error>;

    fn create() -> (Self::Output, u64);
}

impl BuiltinDispatcherFactory for () {
    type Error = ();
    type Output = ();

    fn create() -> (Self::Output, u64) {
        ((), 0)
    }
}

pub trait BuiltinCache {
    fn exists(id: &ProgramId) -> bool;
}

impl BuiltinCache for () {
    fn exists(_id: &ProgramId) -> bool {
        false
    }
}
