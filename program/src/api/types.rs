//! Shared types
use crate::{
    api::{config::GearConfig, generated::api::runtime_types::gear_common::ActiveProgram},
    result::Result,
};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use subxt::{
    events::{EventSubscription, FinalizedEventSub},
    ext::sp_runtime::{generic::Header, traits::BlakeTwo256},
    rpc::Subscription,
    tx::{self, TxInBlock},
    OnlineClient,
};

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

pub type Events =
    EventSubscription<GearConfig, OnlineClient<GearConfig>, Subscription<Header<u32, BlakeTwo256>>>;

pub type FinalizedEvents = EventSubscription<
    GearConfig,
    OnlineClient<GearConfig>,
    FinalizedEventSub<Header<u32, BlakeTwo256>>,
>;

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
