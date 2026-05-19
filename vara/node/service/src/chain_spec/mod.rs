// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear chain configurations

pub use runtime_primitives::{AccountId, AccountPublic, Block};
use sc_chain_spec::ChainSpecExtension;

use serde::{Deserialize, Serialize};

#[cfg(feature = "vara-native")]
pub mod vara;

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    /// Block numbers with known hashes.
    pub fork_blocks: sc_client_api::ForkBlocks<Block>,
    /// Known bad block hashes.
    pub bad_blocks: sc_client_api::BadBlocks<Block>,
    /// The light sync state extension used by the sync-state rpc.
    pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

/// General `ChainSpec` used as a basis for a specialized config.
pub type RawChainSpec = sc_service::GenericChainSpec<Extensions>;
