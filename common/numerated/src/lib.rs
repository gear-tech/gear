#![no_std]

extern crate alloc;

mod interval;
mod numerated;
mod tree;

pub use interval::{Interval, IntoIntervalError, NotEmptyInterval};
pub use numerated::{Bound, BoundValue, Numerated};
pub use tree::{IntervalsTree, VoidsIterator};

pub use num_traits::{
    self,
    bounds::{LowerBounded, UpperBounded},
    CheckedAdd, One, Zero,
};

#[cfg(any(feature = "mock", test))]
pub mod mock;

#[cfg(test)]
mod tests;
