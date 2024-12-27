#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{
    field::{goldilocks_field::GoldilocksField, types::PrimeField64},
    hash::poseidon::Poseidon,
};

/// A prime order field with the features we need to use it as a base field in our argument system.
pub trait RichField: PrimeField64 + Poseidon {}

impl RichField for GoldilocksField {}
