// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! API trait of the chain spec.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_chain_spec::Properties;

#[rpc(client, server)]
pub trait ChainSpecApi {
    /// Get the chain name, as present in the chain specification.
    #[method(name = "chainSpec_v1_chainName")]
    fn chain_spec_v1_chain_name(&self) -> RpcResult<String>;

    /// Get the chain's genesis hash.
    #[method(name = "chainSpec_v1_genesisHash")]
    fn chain_spec_v1_genesis_hash(&self) -> RpcResult<String>;

    /// Get the properties of the chain, as present in the chain specification.
    ///
    /// # Note
    ///
    /// The json whitespaces are not guaranteed to persist.
    #[method(name = "chainSpec_v1_properties")]
    fn chain_spec_v1_properties(&self) -> RpcResult<Properties>;
}
