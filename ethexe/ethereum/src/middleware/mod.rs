// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{AlloyProvider, abi::IMiddleware};
use alloy::{
    primitives::{Address, U256 as AlloyU256},
    providers::{Provider, RootProvider},
};
use anyhow::{Result, anyhow};
use ethexe_common::{Address as LocalAddress, ValidatorsVec};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

type Instance = IMiddleware::IMiddlewareInstance<AlloyProvider>;
type QueryInstance = IMiddleware::IMiddlewareInstance<RootProvider>;

/// Trait for executing elections in the blockchain
#[async_trait::async_trait]
pub trait ElectionProvider: Send + Sync {
    /// Returns a heap-allocated clone of this provider as a trait object.
    fn clone_boxed(&self) -> Box<dyn ElectionProvider>;

    /// Queries the validator set elected at the given timestamp, limited to `max_validators` entries.
    async fn make_election_at(&self, ts: u64, max_validators: u128) -> Result<ValidatorsVec>;
}

impl<T: ElectionProvider> From<T> for Box<dyn ElectionProvider> {
    fn from(provider: T) -> Self {
        provider.clone_boxed()
    }
}

/// Client wrapper around the on-chain `IMiddleware` contract that sends transactions.
///
/// Provides write access (signed provider) alongside a read-only [`MiddlewareQuery`] view.
#[derive(Clone)]
pub struct Middleware {
    instance: Instance,
}

impl Middleware {
    pub(crate) fn new(address: Address, provider: AlloyProvider) -> Self {
        Self {
            instance: Instance::new(address, provider),
        }
    }

    /// Returns the on-chain address of the deployed Middleware contract.
    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.instance.address().0)
    }

    /// Returns a read-only query handle backed by the root (unauthenticated) provider.
    pub fn query(&self) -> MiddlewareQuery {
        MiddlewareQuery(QueryInstance::new(
            *self.instance.address(),
            self.instance.provider().root().clone(),
        ))
    }
}

/// Read-only view of the `IMiddleware` contract, using an unauthenticated root provider.
///
/// Implements [`ElectionProvider`] to retrieve on-chain validator elections without signing.
#[derive(Clone)]
pub struct MiddlewareQuery(QueryInstance);

#[async_trait::async_trait]
impl ElectionProvider for MiddlewareQuery {
    fn clone_boxed(&self) -> Box<dyn ElectionProvider> {
        Box::new(self.clone())
    }

    async fn make_election_at(&self, ts: u64, max_validators: u128) -> Result<ValidatorsVec> {
        let validators = self
            .0
            .makeElectionAt(
                alloy::primitives::Uint::from(ts),
                AlloyU256::from(max_validators),
            )
            .call()
            .await?;

        validators.try_into().map_err(|err| {
            Into::<anyhow::Error>::into(err).context("MiddlewareQuery make_election_at failed")
        })
    }
}

impl MiddlewareQuery {
    /// Constructs a [`MiddlewareQuery`] from an explicit contract address and root provider.
    pub fn from_provider(middleware_address: impl Into<Address>, provider: RootProvider) -> Self {
        Self(QueryInstance::new(middleware_address.into(), provider))
    }

    /// Queries the Router contract address registered in the Middleware contract.
    pub async fn router(&self) -> Result<LocalAddress> {
        Ok(self.0.router().call().await?.into())
    }
}
/// Test double for [`ElectionProvider`] that returns predefined validator sets keyed by timestamp.
///
/// Intended for unit and integration tests where contract calls should be avoided.
#[derive(Clone)]
pub struct MockElectionProvider {
    predefined_election_at: Arc<RwLock<HashMap<u64, ValidatorsVec>>>,
}

#[async_trait::async_trait]
impl ElectionProvider for MockElectionProvider {
    fn clone_boxed(&self) -> Box<dyn ElectionProvider> {
        Box::new(self.clone())
    }

    async fn make_election_at(&self, ts: u64, _max_validators: u128) -> Result<ValidatorsVec> {
        match self.predefined_election_at.read().await.get(&ts).cloned() {
            Some(election_result) => Ok(election_result),
            None => {
                tracing::warn!(timestamp = %ts, "election not found");
                Err(anyhow!("Election not found"))
            }
        }
    }
}

impl MockElectionProvider {
    /// Creates an empty mock with no predefined elections registered yet.
    pub fn new() -> Self {
        Self {
            predefined_election_at: Arc::new(Default::default()),
        }
    }

    /// Registers a validator set to be returned by [`make_election_at`](ElectionProvider::make_election_at) for the given timestamp.
    pub async fn set_predefined_election_at(&self, ts: u64, validators: ValidatorsVec) {
        tracing::trace!(timestamp = ts, validators = ?validators, "set election result");
        self.predefined_election_at
            .write()
            .await
            .insert(ts, validators);
    }
}
