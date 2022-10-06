//! Shared types
use crate::{
    api::{
        config::GearConfig,
        generated::api::{
            runtime_types::{gear_common::ActiveProgram, sp_runtime::DispatchError},
            Event,
        },
    },
    result::Result,
};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use subxt::TransactionInBlock;

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

/// Transaction in block
pub type InBlock<'i> = Result<TransactionInBlock<'i, GearConfig, DispatchError, Event>>;

/// Gear Program
#[derive(Debug, Decode)]
pub enum Program {
    Active(ActiveProgram),
    Terminated,
}
