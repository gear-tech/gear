// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{scheduler::StorageType, *};
pub use gear_core::gas::LockId;

/// An error indicating there is no corresponding enum variant to the one provided
#[derive(Debug)]
pub struct TryFromStorageTypeError;

impl fmt::Display for TryFromStorageTypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Corresponding enum variant not found")
    }
}

impl TryFrom<StorageType> for LockId {
    type Error = TryFromStorageTypeError;

    fn try_from(storage: StorageType) -> Result<Self, Self::Error> {
        match storage {
            StorageType::Mailbox => Ok(Self::Mailbox),
            StorageType::Waitlist => Ok(Self::Waitlist),
            StorageType::Reservation => Ok(Self::Reservation),
            StorageType::DispatchStash => Ok(Self::DispatchStash),
            _ => Err(TryFromStorageTypeError),
        }
    }
}

pub trait LockableTree: Tree {
    /// Locking some value from underlying node balance.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn lock(
        key: impl Into<Self::NodeId>,
        id: LockId,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Unlocking some value from node's locked balance.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn unlock(
        key: impl Into<Self::NodeId>,
        id: LockId,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Unlocking all value from node's locked balance. Returns the actual amount having been unlocked
    /// (wrapped in a `Result`)
    ///
    /// See [`unlock`](Self::unlock) for details.
    fn unlock_all(key: impl Into<Self::NodeId>, id: LockId) -> Result<Self::Balance, Self::Error> {
        let key = key.into();
        let amount = Self::get_lock(key.clone(), id)?;
        Self::unlock(key, id, amount.clone()).map(|_| amount)
    }

    /// Get locked value associated with given id.
    ///
    /// Returns errors in cases of absence associated with given key node,
    /// or if such functionality is forbidden for specific node type:
    /// for example, for `GasNode::ReservedLocal`.
    fn get_lock(key: impl Into<Self::NodeId>, id: LockId) -> Result<Self::Balance, Self::Error>;
}

#[test]
fn lock_id_enum_discriminants_are_consistent() {
    // Important for the [`gsdk::SignedApi`] implementation:
    // the function `migrate_program()` relies on `LockId::Reservation` having discriminant 2
    assert_eq!(0, LockId::Mailbox as usize);
    assert_eq!(1, LockId::Waitlist as usize);
    assert_eq!(2, LockId::Reservation as usize);
    assert_eq!(3, LockId::DispatchStash as usize);
}
