// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear config
//!
//! see <https://github.com/gear-tech/gear/blob/f48450dd9bad2efb9cb3fb13353464ca73e7b7f9/runtime/src/lib.rs#L183>
use subxt::{
    Config,
    config::{
        polkadot::PolkadotExtrinsicParams,
        substrate::{BlakeTwo256, SubstrateHeader},
    },
};

pub type Header = SubstrateHeader<u32, BlakeTwo256>;

/// gear config
///
/// see <https://github.com/gear-tech/gear/blob/f48450dd9bad2efb9cb3fb13353464ca73e7b7f9/runtime/src/lib.rs#L183>
#[derive(Clone, Debug)]
pub struct GearConfig;

impl Config for GearConfig {
    type AssetId = ();
    type Hasher = BlakeTwo256;
    type AccountId = sp_runtime::AccountId32;
    type Address = sp_runtime::MultiAddress<Self::AccountId, ()>;
    type Header = Header;
    type Signature = sp_runtime::MultiSignature;
    type ExtrinsicParams = PolkadotExtrinsicParams<Self>;
}
