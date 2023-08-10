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

pub trait ReservableTree: Tree {
    /// Reserve some value from underlying balance.
    ///
    /// Used in gas reservation feature.
    fn reserve(
        key: impl Into<Self::NodeId>,
        new_key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Reserve some value from underlying balance.
    ///
    /// Used in gas reservation for system signal.
    fn system_reserve(
        key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Unreserve some value from underlying balance.
    ///
    /// Used in gas reservation for system signal.
    fn system_unreserve(key: impl Into<Self::NodeId>) -> Result<Self::Balance, Self::Error>;

    /// Get system reserve value associated with given id.
    ///
    /// Returns errors in cases of absence associated with given key node,
    /// or if such functionality is forbidden for specific node type:
    /// for example, for `GasNode::ReservedLocal`.
    fn get_system_reserve(key: impl Into<Self::NodeId>) -> Result<Self::Balance, Self::Error>;
}
