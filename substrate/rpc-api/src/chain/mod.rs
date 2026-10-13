// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate blockchain API.

pub mod error;

use error::Error;
use jsonrpsee::proc_macros::rpc;
use sp_rpc::{list::ListOrValue, number::NumberOrHex};

#[rpc(client, server)]
pub trait ChainApi<Number, Hash, Header, SignedBlock> {
    /// Get header.
    #[method(name = "chain_getHeader", blocking)]
    fn header(&self, hash: Option<Hash>) -> Result<Option<Header>, Error>;

    /// Get header and body of a block.
    #[method(name = "chain_getBlock", blocking)]
    fn block(&self, hash: Option<Hash>) -> Result<Option<SignedBlock>, Error>;

    /// Get hash of the n-th block in the canon chain.
    ///
    /// By default returns latest block hash.
    #[method(name = "chain_getBlockHash", aliases = ["chain_getHead"], blocking)]
    fn block_hash(
        &self,
        hash: Option<ListOrValue<NumberOrHex>>,
    ) -> Result<ListOrValue<Option<Hash>>, Error>;

    /// Get hash of the last finalized block in the canon chain.
    #[method(name = "chain_getFinalizedHead", aliases = ["chain_getFinalisedHead"], blocking)]
    fn finalized_head(&self) -> Result<Hash, Error>;

    /// All head subscription.
    #[subscription(
		name = "chain_subscribeAllHeads" => "chain_allHead",
		unsubscribe = "chain_unsubscribeAllHeads",
		item = Header
	)]
    fn subscribe_all_heads(&self);

    /// New head subscription.
    #[subscription(
		name = "chain_subscribeNewHeads" => "chain_newHead",
		aliases = ["subscribe_newHead", "chain_subscribeNewHead"],
		unsubscribe = "chain_unsubscribeNewHeads",
		unsubscribe_aliases = ["unsubscribe_newHead", "chain_unsubscribeNewHead"],
		item = Header
	)]
    fn subscribe_new_heads(&self);

    /// Finalized head subscription.
    #[subscription(
		name = "chain_subscribeFinalizedHeads" => "chain_finalizedHead",
		aliases = ["chain_subscribeFinalisedHeads"],
		unsubscribe = "chain_unsubscribeFinalizedHeads",
		unsubscribe_aliases = ["chain_unsubscribeFinalisedHeads"],
		item = Header
	)]
    fn subscribe_finalized_heads(&self);
}
