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

//! Module for counting primitive.
//!
//! Counting primitives are able to return the information
//! about amount of elements they contain.

/// Represents default counting logic, by providing ability
/// to return length of the object as specified (associated) type
/// or answer the question is the object empty.
pub trait Counted {
    /// Returning length type.
    type Length: Default + PartialEq;

    /// Returns current Self's amount of elements as `Length` type.
    fn len() -> Self::Length;

    /// Returns bool, defining if Self doesn't contain elements.
    fn is_empty() -> bool {
        Self::len() == Default::default()
    }
}

/// Represents default counting logic, by providing ability
/// to return length of the object as specified (associated) type
/// or answer the question is the object empty, by provided key of
/// specified (associated) type.
///
/// Should be implemented on double map based types.
pub trait CountedByKey {
    /// Key type of counting target.
    type Key;
    /// Returning length type.
    type Length: Default + PartialEq;

    /// Returns current Self's amount of elements as `Length` type.
    fn len(key: &Self::Key) -> Self::Length;

    /// Returns bool, defining if Self doesn't contain elements.
    fn is_empty(key: &Self::Key) -> bool {
        Self::len(key) == Default::default()
    }
}
