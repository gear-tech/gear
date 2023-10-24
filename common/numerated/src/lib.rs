#![no_std]

extern crate alloc;

mod interval;
mod numerated;
mod tree;

pub use interval::{Interval, IntoIntervalError, NotEmptyInterval};
pub use numerated::{Bound, BoundValue, Numerated};
pub use tree::{Drops, VoidsIterator};

pub use num_traits::{
    self,
    bounds::{LowerBounded, UpperBounded},
    CheckedAdd, One, Zero,
};
