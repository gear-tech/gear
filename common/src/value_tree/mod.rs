use super::*;
use frame_support::{
    traits::{tokens::Balance as BalanceTrait, Imbalance, SameOrOther, TryDrop},
    RuntimeDebug,
};
use sp_runtime::traits::Zero;
use sp_std::{marker::PhantomData, mem};

mod error;
mod negative_imbalance;
mod node;
mod positive_imbalance;

pub use error::Error;
pub use negative_imbalance::NegativeImbalance;
pub use positive_imbalance::PositiveImbalance;

pub use node::{ValueNode, ValueType};

pub struct ValueTreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>(
    PhantomData<(TotalValue, InternalError, Error, ExternalId, StorageMap)>,
);

impl<TotalValue, Balance, InternalError, Error, MapKey, ExternalId, StorageMap>
    ValueTreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>
where
    Balance: BalanceTrait,
    TotalValue: ValueStorage<Value = Balance>,
    InternalError: error::Error,
    Error: From<InternalError>,
    ExternalId: Default + Clone,
    MapKey: Copy,
    StorageMap:
        super::storage::MapStorage<Key = MapKey, Value = ValueNode<ExternalId, MapKey, Balance>>,
{
    fn get_node(key: MapKey) -> Option<StorageMap::Value> {
        StorageMap::get(&key)
    }

    /// The first upstream node (self included), that is able to hold a concrete value, but doesn't
    /// necessarily have a non-zero value.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// Returns tuple of two values, where:
    /// - first value is an ancestor, which has a specified gas amount
    /// - second value is the id of the ancestor.
    /// The latter value is of `Option` type. If it's `None`, it means, that the ancestor and `self`
    /// are the same.
    fn node_with_value(
        node: StorageMap::Value,
    ) -> Result<(StorageMap::Value, Option<MapKey>), Error> {
        let mut ret_node = node;
        let mut ret_id = None;
        while let ValueType::UnspecifiedLocal { parent } = ret_node.inner {
            ret_id = Some(parent);
            ret_node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
        }

        Ok((ret_node, ret_id))
    }

    /// Returns id and data for root node (as [`ValueNode`]) of the value tree, which contains `self` node.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// As in [`ValueNode::node_with_value`], root's id is of `Option` type. It is equal to `None` in case
    /// `self` is a root node.
    pub fn root(node: StorageMap::Value) -> Result<(StorageMap::Value, Option<MapKey>), Error> {
        let mut ret_id = None;
        let mut ret_node = node;
        while let Some(parent) = ret_node.parent() {
            ret_id = Some(parent);
            ret_node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
        }

        Ok((ret_node, ret_id))
    }

    fn decrease_parents_ref(node: &StorageMap::Value) -> Result<(), Error> {
        let id = match node.parent() {
            Some(id) => id,
            None => return Ok(()),
        };

        let mut parent = Self::get_node(id).ok_or_else(InternalError::parent_is_lost)?;
        if parent.refs() == 0 {
            return Err(InternalError::parent_has_no_children().into());
        }

        match node.inner {
            ValueType::SpecifiedLocal { .. } => {
                parent.spec_refs = parent.spec_refs.saturating_sub(1)
            }
            ValueType::UnspecifiedLocal { .. } => {
                parent.unspec_refs = parent.unspec_refs.saturating_sub(1)
            }
            ValueType::External { .. } => {
                unreachable!("node is guaranteed to have a parent, so can't be an external one")
            }
        }

        // Update parent node
        // GasTree::<T>::insert(id, parent);
        StorageMap::insert(id, parent);

        Ok(())
    }

    /// If `self` is of `ValueType::SpecifiedLocal` type, moves value upstream
    /// to the first ancestor, that can hold the value, in case `self` has not
    /// unspec children refs.
    ///
    /// This method is actually one of pre-delete procedures called when node is consumed.
    ///
    /// # Note
    /// Method doesn't mutate `self` in the storage, but only changes it's balance in memory.
    fn move_value_upstream(node: &mut StorageMap::Value) -> Result<(), Error> {
        if let ValueType::SpecifiedLocal {
            value: self_value,
            parent,
        } = node.inner
        {
            if node.unspec_refs == 0 {
                // This is specified, so it needs to get to the first specified parent also
                // going up until external or specified parent is found

                // `parent` key is known to exist, hence there must be it's ancestor with value.
                // If there isn't, the gas tree is considered corrupted (invalidated).
                let (mut parents_ancestor, ancestor_id) = Self::node_with_value(
                    Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?,
                )?;

                // NOTE: intentional expect. A parents_ancestor is guaranteed to have inner_value
                let val = parents_ancestor
                    .inner_value_mut()
                    .expect("Querying parent with value");
                *val = val.saturating_add(self_value);
                *node
                    .inner_value_mut()
                    .expect("self is a type with a specified value") = Zero::zero();

                // GasTree::<T>::insert(ancestor_id.unwrap_or(parent), parents_ancestor);
                StorageMap::insert(ancestor_id.unwrap_or(parent), parents_ancestor);
            }
        }
        Ok(())
    }

    fn check_consumed(
        key: MapKey,
    ) -> Result<crate::ConsumeOutput<NegativeImbalance<Balance, TotalValue>, ExternalId>, Error>
    {
        let mut node_id = key;
        let mut node = Self::get_node(node_id).ok_or_else(InternalError::node_not_found)?;
        while node.consumed && node.refs() == 0 {
            Self::decrease_parents_ref(&node)?;
            Self::move_value_upstream(&mut node)?;
            StorageMap::remove(node_id);

            match node.inner {
                ValueType::External { id, value } => {
                    return Ok(Some((NegativeImbalance::new(value), id)))
                }
                ValueType::SpecifiedLocal { parent, .. }
                | ValueType::UnspecifiedLocal { parent } => {
                    node_id = parent;
                    node = Self::get_node(node_id).ok_or_else(InternalError::parent_is_lost)?;
                }
            }
        }

        Ok(None)
    }
}

impl<TotalValue, Balance, InternalError, Error, MapKey, ExternalId, StorageMap> ValueTree
    for ValueTreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>
where
    Balance: BalanceTrait,
    TotalValue: ValueStorage<Value = Balance>,
    InternalError: error::Error,
    Error: From<InternalError>,
    ExternalId: Default + Clone,
    MapKey: Copy,
    StorageMap:
        super::storage::MapStorage<Key = MapKey, Value = ValueNode<ExternalId, MapKey, Balance>>,
{
    type ExternalOrigin = ExternalId;
    type Key = MapKey;
    type Balance = Balance;

    type PositiveImbalance = PositiveImbalance<Balance, TotalValue>;
    type NegativeImbalance = NegativeImbalance<Balance, TotalValue>;

    type InternalError = InternalError;
    type Error = Error;

    fn total_supply() -> Self::Balance {
        TotalValue::get().unwrap_or_else(Zero::zero)
    }

    fn create(
        origin: Self::ExternalOrigin,
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, Self::Error> {
        if StorageMap::contains_key(&key) {
            return Err(InternalError::node_already_exists().into());
        }

        let node = ValueNode::new(origin, amount);

        // Save value node to storage
        // GasTree::<T>::insert(key, node);
        StorageMap::insert(key, node);

        Ok(PositiveImbalance::new(amount))
    }

    fn get_origin(key: Self::Key) -> Result<Option<Self::ExternalOrigin>, Self::Error> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            let (root, _) = Self::root(node)?;
            if let ValueNode {
                inner: ValueType::External { id, .. },
                ..
            } = root
            {
                Some(id)
            } else {
                unreachable!("Guaranteed by ValueNode::root method");
            }
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_origin_key(key: Self::Key) -> Result<Option<Self::Key>, Self::Error> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            Self::root(node).map(|(_, id)| Some(id.unwrap_or(key)))?
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_limit(key: Self::Key) -> Result<Option<Self::Balance>, Self::Error> {
        if let Some(node) = Self::get_node(key) {
            Ok({
                let (node_with_value, _) = Self::node_with_value(node)?;
                // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
                let v = node_with_value
                    .inner_value()
                    .expect("The node here is either external or specified, hence the inner value");
                Some(v)
            })
        } else {
            Ok(None)
        }
    }

    fn consume(
        key: Self::Key,
    ) -> Result<ConsumeOutput<Self::NegativeImbalance, Self::ExternalOrigin>, Self::Error> {
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        if node.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        node.consumed = true;
        Self::move_value_upstream(&mut node)?;

        Ok(if node.refs() == 0 {
            Self::decrease_parents_ref(&node)?;
            // GasTree::<T>::remove(key);
            StorageMap::remove(key);
            match node.inner {
                ValueType::UnspecifiedLocal { parent }
                | ValueType::SpecifiedLocal { parent, .. } => Self::check_consumed(parent)?,
                ValueType::External { id, value } => Some((NegativeImbalance::new(value), id)),
            }
        } else {
            // Save current node
            // GasTree::<T>::insert(key, node);
            StorageMap::insert(key, node);
            None
        })
    }

    fn spend(
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::NegativeImbalance, Self::Error> {
        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let (mut node, node_id) =
            Self::node_with_value(Self::get_node(key).ok_or_else(InternalError::node_not_found)?)?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        if node.inner_value().expect("Querying node with value") < amount {
            return Err(InternalError::insufficient_balance().into());
        }

        *node.inner_value_mut().expect("Querying node with value") -= amount;
        log::debug!("Spent {:?} of gas", amount);

        // Save node that delivers limit
        // GasTree::<T>::insert(node_id.unwrap_or(key), node);
        StorageMap::insert(node_id.unwrap_or(key), node);

        Ok(NegativeImbalance::new(amount))
    }

    fn split_with_value(
        key: Self::Key,
        new_key: Self::Key,
        amount: Self::Balance,
    ) -> Result<(), Self::Error> {
        let mut parent = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        if parent.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_key) {
            return Err(InternalError::node_already_exists().into());
        }

        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let (mut ancestor_with_value, ancestor_id) = Self::node_with_value(parent.clone())?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        if ancestor_with_value
            .inner_value()
            .expect("Querying node with value")
            < amount
        {
            return Err(InternalError::insufficient_balance().into());
        }

        let new_node = ValueNode {
            inner: ValueType::SpecifiedLocal {
                value: amount,
                parent: key,
            },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        // GasTree::<T>::insert(new_node_key, new_node);
        StorageMap::insert(new_key, new_node);

        parent.spec_refs = parent.spec_refs.saturating_add(1);
        if let Some(ancestor_id) = ancestor_id {
            // Update current node
            // GasTree::<T>::insert(key, parent);
            StorageMap::insert(key, parent);
            *ancestor_with_value
                .inner_value_mut()
                .expect("Querying node with value") -= amount;
            // GasTree::<T>::insert(ancestor_id, ancestor_with_value);
            StorageMap::insert(ancestor_id, ancestor_with_value);
        } else {
            // parent and ancestor nodes are the same
            *parent.inner_value_mut().expect("Querying node with value") -= amount;
            // GasTree::<T>::insert(key, parent);
            StorageMap::insert(key, parent);
        }

        Ok(())
    }

    fn split(key: Self::Key, new_key: Self::Key) -> Result<(), Self::Error> {
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        if node.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_key) {
            return Err(InternalError::node_already_exists().into());
        }

        node.unspec_refs = node.unspec_refs.saturating_add(1);

        let new_node = ValueNode {
            inner: ValueType::UnspecifiedLocal { parent: key },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        // GasTree::<T>::insert(new_node_key, new_node);
        StorageMap::insert(new_key, new_node);
        // Update current node
        // GasTree::<T>::insert(key, node);
        StorageMap::insert(key, node);

        Ok(())
    }
}
