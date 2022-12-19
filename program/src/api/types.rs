//! Shared types
use crate::{
    api::{config::GearConfig, generated::api::runtime_types::gear_common::ActiveProgram},
    result::Result,
};
use futures::{Stream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, marker::Unpin, pin::Pin, result::Result as StdResult, task::Poll};
use subxt::{
    blocks::Block,
    events::Events,
    tx::{self, TxInBlock},
    Error, OnlineClient,
};

pub struct FinalizedBlocks(
    pub Pin<Box<dyn Stream<Item = StdResult<Block<GearConfig, OnlineClient<GearConfig>>, Error>>>>,
);

impl Unpin for FinalizedBlocks {}

impl Stream for FinalizedBlocks {
    type Item = StdResult<Block<GearConfig, OnlineClient<GearConfig>>, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let res = futures::ready!(self.0.poll_next_unpin(cx));

        Poll::Ready(res)
    }
}

impl FinalizedBlocks {
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
