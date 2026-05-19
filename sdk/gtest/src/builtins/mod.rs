// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Builtin actors implementations for gtest.

mod bls12_381;
mod eth_bridge;

pub use bls12_381::{BLS12_381_ID, Bls12_381Request, Bls12_381Response};
pub use eth_bridge::{ETH_BRIDGE_ID, EthBridgeRequest, EthBridgeResponse};

pub(crate) use bls12_381::BlsOpsGasCostsImpl;
pub(crate) use eth_bridge::process_eth_bridge_dispatch;
