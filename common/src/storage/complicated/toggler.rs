// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Module for toggler/flag implementation.
//!
//! Toggler provides API for allowing or denying actions.
//! Could be used to branch logic by toggler condition.

use crate::storage::ValueStorage;
use core::marker::PhantomData;

/// Represents logic of providing access for some actions.
pub trait Toggler {
    /// Sets condition to allowed for some action.
    fn allow();

    /// Returns bool, defining does toggle allow some action.
    fn allowed() -> bool;

    /// Returns bool, defining does toggle deny some action.
    ///
    /// Represents `Self::allowed` inversion.
    fn denied() -> bool {
        !Self::allowed()
    }

    /// Sets condition to denied for some action.
    fn deny();
}

/// `Toggler` implementation based on `ValueStorage`.
pub struct TogglerImpl<VS: ValueStorage>(PhantomData<VS>);

// `Toggler` implementation over `ValueStorage` of `bool` storing type.
impl<VS: ValueStorage<Value = bool>> Toggler for TogglerImpl<VS> {
    fn allow() {
        VS::put(true);
    }

    fn allowed() -> bool {
        VS::get() != Some(false)
    }

    fn deny() {
        VS::put(false);
    }
}
