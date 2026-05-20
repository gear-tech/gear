// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Modified implementation of the basic block-authorship logic from
// https://github.com/paritytech/substrate/tree/master/client/basic-authorship.
// The block proposer explicitly pushes the `pallet_gear::run`
// extrinsic at the end of each block.

#![allow(clippy::items_after_test_module)]

mod authorship;
mod block_builder;

#[cfg(test)]
mod tests;

pub use crate::authorship::{
    DEFAULT_BLOCK_SIZE_LIMIT, DEFAULT_DEADLINE_SLIPPAGE, DEFAULT_DISPATCH_RATIO, Proposer,
    ProposerFactory,
};
