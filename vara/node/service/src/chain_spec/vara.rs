// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::chain_spec::Extensions;
use gear_runtime_common::{
    self,
    constants::{VARA_DECIMAL, VARA_SS58PREFIX, VARA_TESTNET_TOKEN_SYMBOL},
};
use sc_chain_spec::{DEV_RUNTIME_PRESET, LOCAL_TESTNET_RUNTIME_PRESET, Properties};
use sc_service::ChainType;
use vara_runtime::WASM_BINARY;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

/// Returns the [`Properties`] for the Vara Network Testnet (dev runtime).
pub fn vara_dev_properties() -> Properties {
    let mut p = Properties::new();

    p.insert("ss58format".into(), VARA_SS58PREFIX.into());
    p.insert("tokenDecimals".into(), VARA_DECIMAL.into());
    p.insert("tokenSymbol".into(), VARA_TESTNET_TOKEN_SYMBOL.into());

    p
}

pub fn development_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Development")
        .with_id("vara_dev")
        .with_chain_type(ChainType::Development)
        .with_genesis_config_preset_name(DEV_RUNTIME_PRESET)
        .with_properties(vara_dev_properties())
        .build())
}

pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Local test wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Vara Local Testnet")
        .with_id("vara_local_testnet")
        .with_chain_type(ChainType::Local)
        .with_genesis_config_preset_name(LOCAL_TESTNET_RUNTIME_PRESET)
        .with_properties(vara_dev_properties())
        .build())
}
