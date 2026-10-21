// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(non_snake_case)]

//! API trait of the chain head.
use crate::{
    chain_head::{
        error::Error,
        event::{FollowEvent, MethodResponse},
    },
    common::events::StorageQuery,
};
use jsonrpsee::{proc_macros::rpc, server::ResponsePayload};
pub use sp_rpc::list::ListOrValue;

#[rpc(client, server)]
pub trait ChainHeadApi<Hash> {
    /// Track the state of the head of the chain: the finalized, non-finalized, and best blocks.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[subscription(
		name = "chainHead_v1_follow" => "chainHead_v1_followEvent",
		unsubscribe = "chainHead_v1_unfollow",
		item = FollowEvent<Hash>,
	)]
    fn chain_head_unstable_follow(&self, with_runtime: bool);

    /// Retrieves the body (list of transactions) of a pinned block.
    ///
    /// This method should be seen as a complement to `chainHead_v1_follow`,
    /// allowing the JSON-RPC client to retrieve more information about a block
    /// that has been reported.
    ///
    /// Use `archive_unstable_body` if instead you want to retrieve the body of an arbitrary block.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_body", with_extensions)]
    async fn chain_head_unstable_body(
        &self,
        follow_subscription: String,
        hash: Hash,
    ) -> ResponsePayload<'static, MethodResponse>;

    /// Retrieves the header of a pinned block.
    ///
    /// This method should be seen as a complement to `chainHead_v1_follow`,
    /// allowing the JSON-RPC client to retrieve more information about a block
    /// that has been reported.
    ///
    /// Use `archive_unstable_header` if instead you want to retrieve the header of an arbitrary
    /// block.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_header", with_extensions)]
    async fn chain_head_unstable_header(
        &self,
        follow_subscription: String,
        hash: Hash,
    ) -> Result<Option<String>, Error>;

    /// Returns storage entries at a specific block's state.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_storage", with_extensions)]
    async fn chain_head_unstable_storage(
        &self,
        follow_subscription: String,
        hash: Hash,
        items: Vec<StorageQuery<String>>,
        child_trie: Option<String>,
    ) -> ResponsePayload<'static, MethodResponse>;

    /// Call into the Runtime API at a specified block's state.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_call", with_extensions)]
    async fn chain_head_unstable_call(
        &self,
        follow_subscription: String,
        hash: Hash,
        function: String,
        call_parameters: String,
    ) -> ResponsePayload<'static, MethodResponse>;

    /// Unpin a block or multiple blocks reported by the `follow` method.
    ///
    /// Ongoing operations that require the provided block
    /// will continue normally.
    ///
    /// When this method returns an error, it is guaranteed that no blocks have been unpinned.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_unpin", with_extensions)]
    async fn chain_head_unstable_unpin(
        &self,
        follow_subscription: String,
        hash_or_hashes: ListOrValue<Hash>,
    ) -> Result<(), Error>;

    /// Resumes a storage fetch started with `chainHead_storage` after it has generated an
    /// `operationWaitingForContinue` event.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_continue", with_extensions)]
    async fn chain_head_unstable_continue(
        &self,
        follow_subscription: String,
        operation_id: String,
    ) -> Result<(), Error>;

    /// Stops an operation started with chainHead_v1_body, chainHead_v1_call, or
    /// chainHead_v1_storage. If the operation was still in progress, this interrupts it. If
    /// the operation was already finished, this call has no effect.
    ///
    /// # Unstable
    ///
    /// This method is unstable and subject to change in the future.
    #[method(name = "chainHead_v1_stopOperation", with_extensions)]
    async fn chain_head_unstable_stop_operation(
        &self,
        follow_subscription: String,
        operation_id: String,
    ) -> Result<(), Error>;
}
