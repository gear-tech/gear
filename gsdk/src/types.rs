// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Shared types
use crate::{
    config::GearConfig,
    metadata::runtime_types::gear_common::{
        gas_provider::node::{GasNode, GasNodeId},
        ActiveProgram,
    },
    result::Result,
};
use futures::{Stream, StreamExt};
use gear_core::ids::*;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sp_runtime::AccountId32;
use std::{collections::HashMap, marker::Unpin, pin::Pin, result::Result as StdResult, task::Poll};
use subxt::{
    blocks::Block,
    events::Events,
    tx::{self, TxInBlock},
    Error, OnlineClient,
};

/// Subscription of finalized blocks.
#[allow(clippy::type_complexity)]
pub struct Blocks(
    pub Pin<Box<dyn Stream<Item = StdResult<Block<GearConfig, OnlineClient<GearConfig>>, Error>>>>,
);

impl Unpin for Blocks {}

impl Stream for Blocks {
    type Item = StdResult<Block<GearConfig, OnlineClient<GearConfig>>, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let res = futures::ready!(self.0.poll_next_unpin(cx));

        Poll::Ready(res)
    }
}

impl Blocks {
    /// Wait for the next item from the subscription.
    pub async fn next_events(&mut self) -> Option<StdResult<Events<GearConfig>, Error>> {
        if let Some(block) = StreamExt::next(self).await {
            Some(block.ok()?.events().await)
        } else {
            None
        }
    }
}

/// Information of gas
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, Serialize, Deserialize)]
pub struct GasInfo {
    /// Represents minimum gas limit required for execution.
    pub min_limit: u64,
    /// Gas amount that we reserve for some other on-chain interactions.
    pub reserved: u64,
    /// Contains number of gas burned during message processing.
    pub burned: u64,
}

/// Gear gas node id.
pub type GearGasNodeId = GasNodeId<MessageId, ReservationId>;

/// Gear gas node.
pub type GearGasNode = GasNode<AccountId32, GearGasNodeId, u64>;

/// Gear pages.
pub type GearPages = HashMap<u32, Vec<u8>>;

/// Transaction in block.
pub type InBlock = Result<TxInBlock<GearConfig, OnlineClient<GearConfig>>>;

/// Transaction status.
pub type TxStatus = tx::TxStatus<GearConfig, OnlineClient<GearConfig>>;

/// Gear Program
#[derive(Debug, Decode)]
pub enum Program {
    Active(ActiveProgram),
    Terminated,
}
