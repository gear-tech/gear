pub(crate) mod complex;
pub(crate) mod complicated;
pub(crate) mod primitives;

pub use complex::*;
pub use complicated::*;
pub use primitives::*;

mod counter;
mod deque;
mod flag;
mod map;
mod messenger;
mod value;

pub use counter::{StorageCounter, TargetedStorageCounter};
pub use deque::{DequeError, NextKey, Node, StorageDeque};
pub use flag::{StorageFlag, TargetedStorageFlag};
pub use map::{StorageMap, TargetedStorageMap};
pub use messenger::Messenger;
pub use value::{StorageValue, TargetedStorageValue};

/// Callback trait for running some logic depent on conditions.
pub trait Callback<T, R = ()> {
    fn call(arg: &T) -> R;
}

/// Empty implementation for skipping callback.
impl<T> Callback<T> for () {
    fn call(_: &T) {}
}
