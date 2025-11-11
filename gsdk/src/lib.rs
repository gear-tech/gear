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

#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Gear api
pub use crate::{
    api::{Api, ApiBuilder},
    config::GearConfig,
    convert::{IntoSubstrate, IntoSubxt},
    gear::Event,
    result::{Error, Result},
    signer::PairSigner,
    subscription::{
        Blocks, Events, PayloadFilter, ProgramStateChange, ProgramStateChanges, UserMessageSent,
        UserMessageSentFilter, UserMessageSentSubscription,
    },
};
pub use gear_core::rpc::GasInfo;
pub use subxt::{self, dynamic::Value};

use crate::{
    gear::runtime_types::gear_common::gas_provider::node::{GasNode, GasNodeId},
    metadata::runtime_types::gear_core::program::ActiveProgram,
};
use gear_core::{
    ids::{MessageId, ReservationId},
    memory::PageBuf,
};
use parity_scale_codec::Decode;
use std::collections::HashMap;
use subxt::{
    OnlineClient,
    tx::{TxInBlock as SubxtTxInBlock, TxStatus as SubxtTxStatus},
};

/// Generated runtime API types.
// FIXME: substitute `gear_core::page::Page`,
//        requires `subxt` to support const parameters.
#[subxt::subxt(
    runtime_metadata_path = "vara_runtime.scale",
    derive_for_all_types = "Clone, ::subxt::ext::codec::Encode, ::subxt::ext::codec::Decode",
    substitute_type(
        path = "sp_arithmetic::per_things::Percent",
        with = "::subxt::utils::Static<::sp_runtime::Percent>"
    ),
    substitute_type(path = "gprimitives::CodeId", with = "::gear_core::ids::CodeId"),
    substitute_type(path = "gprimitives::MessageId", with = "::gear_core::ids::MessageId"),
    substitute_type(path = "gprimitives::ActorId", with = "::gear_core::ids::ActorId"),
    substitute_type(
        path = "gprimitives::ReservationId",
        with = "::gear_core::ids::ReservationId"
    ),
    substitute_type(
        path = "gear_core::program::MemoryInfix",
        with = "::gear_core::program::MemoryInfix"
    ),
    substitute_type(
        path = "gear_core::memory::PageBuf",
        with = "::gear_core::memory::PageBuf"
    ),
    generate_docs
)]
pub mod gear {}

mod api;
pub mod backtrace;
pub mod config;
mod constants;
mod convert;
pub mod events;
pub mod metadata;
pub mod result;
mod rpc;
pub mod signer;
mod storage;
pub mod subscription;
mod tx_status;
mod utils;

mod ensure_versions;

pub mod ext {
    pub use sp_core;
    pub use sp_runtime::{self, codec, scale_info};
    pub use subxt::utils;
}
pub mod gp {
    //! generated code preludes.
    pub use subxt::ext::{
        codec::{Decode, Encode},
        scale_decode::DecodeAsType,
        scale_encode::EncodeAsType,
    };
}

/// Block number type
pub type BlockNumber = u32;

/// Gear gas node id.
pub type GearGasNodeId = GasNodeId<MessageId, ReservationId>;

/// Gear gas node.
pub type GearGasNode = GasNode<subxt::utils::AccountId32, GearGasNodeId, u64, u128>;

/// Gear pages.
pub type GearPages = HashMap<u32, PageBuf>;

/// Transaction in block.
pub type TxInBlock = SubxtTxInBlock<GearConfig, OnlineClient<GearConfig>>;

/// Transaction in block with result wrapper.
pub type TxInBlockResult = Result<TxInBlock>;

/// Transaction status.
pub type TxStatus = SubxtTxStatus<GearConfig, OnlineClient<GearConfig>>;

/// Gear Program
#[derive(Debug, Decode)]
pub enum Program {
    Active(ActiveProgram<BlockNumber>),
    Terminated,
}
