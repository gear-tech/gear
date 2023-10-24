#![no_std]

extern crate alloc;

mod interval;
mod seq;
mod tree;

pub use interval::{Interval, IntoIntervalError, NotEmptyInterval};
pub use seq::{Bound, BoundValue, Numerated};
pub use tree::{Drops, VoidsIterator};

pub use num_traits::{
    self,
    bounds::{LowerBounded, UpperBounded},
    CheckedAdd, One, Zero,
};
