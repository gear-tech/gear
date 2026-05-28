// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
