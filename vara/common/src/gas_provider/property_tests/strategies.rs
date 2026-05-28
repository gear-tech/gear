// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use proptest::prelude::*;

const MAX_ACTIONS: usize = 1000;

/// Enum representing gas tree functions that mutate the state of the existing
/// gas tree.
///
/// Instead of `H256` values, `usize` values are used to represent ids of nodes
/// in the tree. These ids are like **handles** pointing to some existing id in
/// the tree. It's used the next way:
/// ```no_run
/// let node_ids: Vec<H256> = Vec::new();
///
/// // ...
/// let action = GasTreeAction::Split(12312312);
///
/// // ...
///
/// if let GasTreeAction::Split(parent_idx) = action {
///     // For `ring_get` logic details, see `RingGet` trait implementation.
///     let parent_id = node_ids.ring_get(parent_idx).unwrap();
///     Gas::split(parent_id, H256::random())
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub(super) enum GasTreeAction {
    /// Performs split of the node under the bound `usize` index.
    Split(usize),
    /// Performs split of the node under the bound `usize` index with `u64`
    /// amount.
    SplitWithValue(usize, u64),
    /// Spends `u64` amount of value from the node under the bound `usize`
    /// index.
    Spend(usize, u64),
    /// Consumes the node under the bound `usize` index.
    Consume(usize),
    /// Cut the value from the node under `usize` index with `u64` amount.
    Cut(usize, u64),
    /// Create gas reservation using `usize` node index with `u64` amount.
    Reserve(usize, u64),
    /// Create lock using `usize` node index with `u64` amount.
    Lock(usize, u64),
    /// Remove lock using `usize` node index with `u64` amount.
    Unlock(usize, u64),
    /// Create system gas reservation using `usize` node index with `u64` amount.
    SystemReserve(usize, u64),
    /// Remove system gas reservation using `usize` node.
    SystemUnreserve(usize),
}

/// Returns random vector of `GasTreeAction`s with a tree's root max balance.
///
/// Execution of the random set of `GasTreeAction`s results in a unique gas tree
/// in the storage, which is needed to perform property tests. Max balance sets
/// upper boundary on the amount by which node's value can be decreased (in
/// split_with_value and spend procedures). Also max balance defines root's
/// balance.
pub(super) fn gas_tree_props_test_strategy() -> impl Strategy<Value = (u64, Vec<GasTreeAction>)> {
    any::<u64>()
        .prop_flat_map(|max_balance| (Just(max_balance), gas_tree_action_strategy(max_balance)))
}

/// Generates random vector of `GasTreeAction`s that defines
/// how gas tree will be created.
pub(super) fn gas_tree_action_strategy(
    max_balance: u64,
) -> impl Strategy<Value = Vec<GasTreeAction>> {
    let action_random_variant = (any::<usize>(), 0..max_balance).prop_flat_map(|(id, amount)| {
        prop_oneof![
            Just(GasTreeAction::SplitWithValue(id, amount)),
            Just(GasTreeAction::Spend(id, amount)),
            Just(GasTreeAction::Cut(id, amount)),
            Just(GasTreeAction::Consume(id)),
            Just(GasTreeAction::Split(id)),
            Just(GasTreeAction::Reserve(id, amount)),
            Just(GasTreeAction::Lock(id, amount)),
            Just(GasTreeAction::Unlock(id, amount)),
            Just(GasTreeAction::SystemReserve(id, amount)),
            Just(GasTreeAction::SystemUnreserve(id))
        ]
    });
    prop::collection::vec(action_random_variant, 0..MAX_ACTIONS)
}
