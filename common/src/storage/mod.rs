mod deck;
mod map;
mod value;

pub use deck::StorageDeck;
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

pub trait NextKey<V> {
    fn first(target: &V) -> Self;
    fn next(&self, target: &V) -> Self;
}

pub struct Node<K, V> {
    pub next: Option<K>,
    pub value: V,
}

pub trait Counter: Default {
    fn increase(&mut self);
    fn decrease(&mut self);
    fn reset(&mut self) {
        *self = Default::default();
    }
}

macro_rules! impl_counter {
    ($($t: ty), *) => {$(
        impl Counter for $t {
            fn increase(&mut self) {
                *self = self.saturating_add(1);
            }

            fn decrease(&mut self) {
                *self = self.saturating_sub(1);
            }
        }
    )*};
}

impl<T: Counter> Counter for Option<T> {
    fn increase(&mut self) {
        if let Some(v) = self {
            v.increase();
        } else {
            let mut default: T = Default::default();
            default.increase();

            *self = Some(default);
        }
    }

    fn decrease(&mut self) {
        if let Some(v) = self {
            v.decrease();
        } else {
            self.reset();
        }
    }

    fn reset(&mut self) {
        *self = Some(Default::default());
    }
}

// Unsigned integers
impl_counter!(u8, u16, u32, u64, u128);

// Signed integers
impl_counter!(i8, i16, i32, i64, i128);
