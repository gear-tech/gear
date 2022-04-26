mod counter;
mod deck;
mod map;
mod value;

pub use counter::{StorageCounter, TargetedStorageCounter};
pub use deck::{StorageDeck, NextKey, Node};
pub use map::{StorageMap, TargetedStorageMap};
pub use value::{StorageValue, TargetedStorageValue};

/// Callback trait for running some logic depent on conditions.
pub trait Callback<T, R = ()> {
    fn call(arg: &T) -> R;
}

/// Empty implementation for skipping callback.
impl<T> Callback<T> for () {
    fn call(_: &T) {}
}
