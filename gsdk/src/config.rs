// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
