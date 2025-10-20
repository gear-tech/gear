// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! This module contains logic to work with finalized blocks.
//! It provides a two main types:
//! - [`FinalizedBlocksStream`] - a stream that yields finalized blocks as they become available in RPC.
//! - [`FinalizedDataSync`] - a type that syncs necessary data for each finalized block.

use crate::RuntimeConfig;
use alloy::{
    consensus::BlockHeader as _,
    eips::BlockNumberOrTag,
    network::{BlockResponse, Ethereum, Network, primitives::HeaderResponse},
    providers::{Provider, RootProvider},
    rpc::types::eth::Block,
    transports::{RpcError, TransportErrorKind},
};
use anyhow::Result;
use ethexe_common::{
    self, Address, OperatorStakingInfo,
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
};
use ethexe_ethereum::{middleware::MiddlewareQuery, primitives::private::derive_more};
use futures::{FutureExt, Stream, future::BoxFuture};
use gprimitives::{H256, U256};
use std::{
    collections::BTreeMap,
    future::IntoFuture,
    ops::Mul,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

// TODO: remove unused traits in SyncDB
pub(crate) trait SyncDB:
    OnChainStorageRead + OnChainStorageWrite + BlockMetaStorageRead + BlockMetaStorageWrite + Clone
{
}
impl<
    T: OnChainStorageRead + OnChainStorageWrite + BlockMetaStorageRead + BlockMetaStorageWrite + Clone,
> SyncDB for T
{
}

type GetBlockFuture<N> =
    BoxFuture<'static, Result<Option<<N as Network>::BlockResponse>, RpcError<TransportErrorKind>>>;

/// [`FinalizedBlocksStream`] returns finalized blocks as they become available.
/// It designed to minimize the number of requests to the provider.
/// It does so by:
/// - Waiting for the approximate time when the next finalized block is expected to be available.
/// - If the block is not yet available, it waits for the next slot duration before trying
/// NOTE: This is not a standart stream provided by the RPC node.
///
/// This type implements a [`Stream`] trait with item [`Network::BlockResponse`]. It doesn't return an error,
/// because it handles errors internally by retrying the requests after some delay.
pub(crate) struct FinalizedBlocksStream<P, N: Network = Ethereum> {
    // Control flow futures
    get_block_fut: Option<GetBlockFuture<N>>,
    sleep_fut: Option<BoxFuture<'static, ()>>,
    // The latest finalized block we have seen
    latest_finalized: Option<N::BlockResponse>,
    // Ethereum slot duration in seconds.
    slot_duration_secs: u64,
    provider: P,
}

impl<P: Provider<N> + Clone, N: Network> FinalizedBlocksStream<P, N> {
    pub fn new(provider: P) -> Self {
        Self {
            get_block_fut: None,
            sleep_fut: None,
            latest_finalized: None,
            slot_duration_secs: alloy::eips::merge::SLOT_DURATION_SECS,
            provider,
        }
    }

    /// Create a mock instance for tests with a custom slot duration.
    #[cfg(test)]
    pub fn mock_new(provider: P, slot_duration_secs: u64) -> Self {
        Self {
            get_block_fut: None,
            sleep_fut: None,
            latest_finalized: None,
            slot_duration_secs,
            provider,
        }
    }

    // Set up a future to wait until the next finalized block is expected to be available.
    fn wait_for_next_finalized_block(&mut self) {
        let current_ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => unreachable!("Block timestamp can not be earlier than UNIX_EPOCH"),
        };

        let secs_to_wait = match &self.latest_finalized {
            Some(block) => {
                let time_spent = current_ts.saturating_sub(block.header().timestamp());
                self.slot_duration_secs.mul(32).saturating_sub(time_spent)
            }
            None => {
                // If we don't have a finalized block yet, we don't need to wait
                0
            }
        };

        // Wait for next finalized block (last block in next Ethereum epoch).
        self.wait_for(Duration::from_secs(secs_to_wait));
    }

    // Wait for the next slot duration before trying again.
    fn wait_for_next_slot(&mut self) {
        self.wait_for(Duration::from_secs(self.slot_duration_secs));
    }

    // Set up a future to wait.
    fn wait_for(&mut self, duration: Duration) {
        self.sleep_fut = Some(tokio::time::sleep(duration).into_future().boxed());
    }
}

impl<P, N> Stream for FinalizedBlocksStream<P, N>
where
    P: Provider<N> + Clone + std::marker::Unpin,
    N: Network,
{
    type Item = N::BlockResponse;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // let _span_guard =
        //     tracing::span!(tracing::Level::TRACE, "FinalizedBlocksStream::poll_next").entered();

        let this = self.as_mut().get_mut();

        if let Some(fut) = this.sleep_fut.as_mut() {
            let _: () = futures::ready!(fut.poll_unpin(cx));
            this.sleep_fut = None;

            let fut = this
                .provider
                .clone()
                .get_block_by_number(BlockNumberOrTag::Finalized)
                .into_future()
                .boxed();
            this.get_block_fut = Some(fut);
        }

        let get_block_fut = match this.get_block_fut.as_mut() {
            Some(fut) => fut,
            None => {
                this.get_block_fut = Some(
                    this.provider
                        .get_block_by_number(BlockNumberOrTag::Finalized)
                        .into_future()
                        .boxed(),
                );
                this.get_block_fut.as_mut().unwrap()
            }
        };

        let get_block_result = futures::ready!(get_block_fut.poll_unpin(cx));
        this.get_block_fut = None;

        let maybe_block = match get_block_result {
            Ok(maybe_block) => maybe_block,
            Err(RpcError::Transport(err)) => match err {
                TransportErrorKind::BackendGone => {
                    // The backend connection stopped, so we need to re-establish connection for provider.
                    unimplemented!("Re-establish provider connection");
                }
                TransportErrorKind::Custom(err) => {
                    tracing::error!(err = %err, "Custom transport error while fetching finalized block, retrying...");
                    self.wait_for_next_slot();
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                TransportErrorKind::HttpError(err) => {
                    tracing::error!(err = %err, "HTTP error while fetching finalized block, retrying...");
                    self.wait_for_next_slot();
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                TransportErrorKind::PubsubUnavailable
                | TransportErrorKind::MissingBatchResponse(_) => {
                    unreachable!()
                }
                _ => {
                    // #[non_exhaustive] in TransportErrorKind - may be added new variants in the future
                    unimplemented!("Received unexpected transport error: {err:?}")
                }
            },
            Err(RpcError::SerError(err)) => {
                todo!()
            }
            Err(RpcError::DeserError { err, text }) => {
                todo!()
            }
            Err(RpcError::LocalUsageError(err)) => {
                unreachable!("Local usage error: {err}");
            }
            Err(RpcError::NullResp) => {
                unreachable!("Null response error");
            }
            Err(RpcError::ErrorResp(err)) => {
                unreachable!("RPC error response: {err:?}");
            }
            Err(RpcError::UnsupportedFeature(err)) => {
                unreachable!("Unsupported feature error: {err}");
            }
        };

        let block = match (maybe_block, this.latest_finalized.clone()) {
            // Returns the fetched block if it's different from the last one we have seen.
            (Some(block), Some(prev_finalized))
                if block.header().hash() != prev_finalized.header().hash() =>
            {
                block
            }
            // If we don't have a finalized block yet, return the fetched one.
            (Some(block), None) => block,
            // RPC returned no block.
            _ => {
                log::trace!("Finalized block not found in RPC");
                // Wait for the next slot and try again.
                this.wait_for_next_slot();
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        this.latest_finalized = Some(block.clone());
        this.wait_for_next_finalized_block();
        cx.waker().wake_by_ref();
        return Poll::Ready(Some(block));
    }
}

/// [`FinalizedDataSync`] works with finalized blocks and sync the necessary data.
#[derive(Clone, derive_more::Debug)]
pub(crate) struct FinalizedDataSync<DB: Clone> {
    #[debug(skip)]
    pub db: DB,
    pub provider: RootProvider,
    pub config: RuntimeConfig,
}

impl<DB: SyncDB> FinalizedDataSync<DB> {
    /// Entry point to process finalized block.
    pub async fn process_finalized_block(self, finalized_block: Block) -> Result<H256> {
        if self.can_load_staking_data(&finalized_block) {
            let _: () = self.sync_staking_data(&finalized_block).await?;
        }

        let finalized_block_hash = finalized_block.header().hash().0.into();
        // self.db
        //     .set_latest_synced_finalized_block(finalized_block_hash);
        Ok(finalized_block_hash)
    }

    // Check if we can load staking data for the given finalized block.
    // We can load staking data if received block in a new era and not in the genesis.
    pub fn can_load_staking_data(&self, block: &Block) -> bool {
        let parent_hash = block.header().parent_hash().0.into();
        let parent = match self.db.block_header(parent_hash) {
            Some(header) => header,
            None => {
                log::error!("Block header not found for parent hash of finalized block");
                return false;
            }
        };

        let era =
            (block.header().timestamp() - self.config.genesis_timestamp) / self.config.era_duration;
        let parent_era =
            (parent.timestamp - self.config.genesis_timestamp) / self.config.era_duration;

        era > 0 && era > parent_era
    }

    // Sync staking data for the given finalized block.
    async fn sync_staking_data(&self, block: &Block) -> Result<()> {
        let middleware_query = MiddlewareQuery::from_provider(
            self.config.middleware_address.0.into(),
            self.provider.root().clone(),
        );

        let validators = self
            .db
            .validators(block.header().hash().0.into())
            .expect("Must be propagate in `propagate_validators`");

        let mut operators_info = BTreeMap::new();
        for validator in validators.iter() {
            // THINK: maybe timestamp not from header
            let validator_stake = middleware_query
                .operator_stake_at(validator.0.into(), block.header().timestamp())
                .await?;

            let validator_stake_vaults = middleware_query
                .operator_stake_vaults_at(validator.0.into(), block.header().timestamp())
                .await?
                .iter()
                .map(|vault_with_stake| {
                    (
                        Address(vault_with_stake.vault.into_array()),
                        U256::from_little_endian(vault_with_stake.stake.as_le_slice()),
                    )
                })
                .collect();

            operators_info.insert(
                *validator,
                OperatorStakingInfo {
                    stake: U256(validator_stake.into_limbs()),
                    staked_vaults: validator_stake_vaults,
                },
            );
        }

        let era =
            (block.header().timestamp() - self.config.genesis_timestamp) / self.config.era_duration;

        self.db.mutate_staking_metadata(era, |metadata| {
            metadata.operators_info = operators_info;
        });

        Ok(())
    }
}
