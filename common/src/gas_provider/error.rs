// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

/// Errors stating that gas tree has been invalidated.
///
/// Contains constructors for all existing errors.
pub trait Error {
    /// Gas (gas tree) has already been created for the provided key.
    fn node_already_exists() -> Self;

    /// Parent must be in the tree, but not found.
    ///
    /// This differs from `node_not_found`, because parent
    /// node for local node types must be found, but was not. Thus,
    /// tree is invalidated.
    fn parent_is_lost() -> Self;

    /// Parent node must have children, but they weren't found.
    ///
    /// If node is a parent to some other node it must have at least
    /// one child, otherwise it's id can't be used as a parent for
    /// local nodes in the tree.
    fn parent_has_no_children() -> Self;

    /// Value node doesn't exist for a key.
    fn node_not_found() -> Self;

    /// Procedure can't be called on consumed node.
    fn node_was_consumed() -> Self;

    /// Account doesn't have enough funds to complete operation.
    fn insufficient_balance() -> Self;

    /// Forbidden operation for the value node.
    fn forbidden() -> Self;

    /// Output of `Tree::consume` procedure that wasn't expected.
    ///
    /// Outputs of consumption procedure are determined. The error is returned
    /// when unexpected one occurred. That signals, that algorithm works wrong
    /// and expected invariants are not correct.
    fn unexpected_consume_output() -> Self;

    /// Node type that can't occur if algorithm work well
    fn unexpected_node_type() -> Self;

    /// Value must have been caught, but was missed or blocked
    /// (see `TreeImpl::catch_value` for details).
    fn value_is_not_caught() -> Self;

    /// Value must have been caught or moved upstream, but was blocked
    /// (see `TreeImpl::catch_value` for details).
    fn value_is_blocked() -> Self;

    /// Value must have been blocked, but was either moved or caught
    /// (see `TreeImpl::catch_value` for details).
    fn value_is_not_blocked() -> Self;

    /// `GasTree::consume` called on node, which has some balance locked.
    fn consumed_with_lock() -> Self;

    /// `GasTree::consume` called on node, which has some system reservation.
    fn consumed_with_system_reservation() -> Self;

    /// `GasTree::create` called with some value amount leading to
    /// the total value overflow.
    fn total_value_is_overflowed() -> Self;

    /// Either `GasTree::consume` or `GasTree::spent` called on a node creating
    /// negative imbalance which leads to the total value drop below 0.
    fn total_value_is_underflowed() -> Self;
}
