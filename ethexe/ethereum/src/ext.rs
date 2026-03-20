use crate::{IntoBlockId, abi::IMirror::stateHashCall as StateHashCall};
use alloy::{
    network::Network as AlloyNetwork,
    primitives::{Address as AlloyAddress, Bytes, TxKind},
    providers::{Provider as _, RootProvider},
    rpc::{
        client::{BatchRequest, Waiter},
        types::{BlockId, TransactionInput, TransactionRequest},
    },
    sol_types::SolCall,
};
use anyhow::{Context as _, Result, ensure};
use futures::{StreamExt as _, stream::FuturesUnordered};
use gprimitives::{ActorId, H256};
use gsigner::Address;
use std::collections::BTreeMap;

const STATE_HASH_BATCH_SIZE: usize = 128;
const MAX_CONCURRENT_BATCHES: usize = 4;

#[allow(async_fn_in_trait)]
pub trait ProviderExt {
    /// Collects the state hashes of the given mirror actors at the specified block.
    /// Returns a mapping from actor ID to its state hash.
    /// Batches the JSON-RPC calls to avoid making one request per mirror, which can be very slow when syncing many programs.
    /// max batch size: [`STATE_HASH_BATCH_SIZE`] , max concurrent batches: [`MAX_CONCURRENT_BATCHES`]
    async fn collect_mirror_states(
        &self,
        at: impl IntoBlockId,
        mirrors: Vec<ActorId>,
    ) -> Result<BTreeMap<ActorId, H256>>;
}

impl<N: AlloyNetwork> ProviderExt for RootProvider<N> {
    async fn collect_mirror_states(
        &self,
        at: impl IntoBlockId,
        mirrors: Vec<ActorId>,
    ) -> Result<BTreeMap<ActorId, H256>> {
        let mut program_states = BTreeMap::new();
        let block_id = at.into_block_id();

        log::trace!(
            "Collecting state hashes for {} mirror actors at block {block_id}",
            mirrors.len(),
        );

        let mut futures = Vec::new();
        for chunk in mirrors.chunks(STATE_HASH_BATCH_SIZE) {
            futures.push(collect_mirror_states_batch(self.clone(), block_id, chunk));
        }

        let mut futures_unordered: FuturesUnordered<_> = futures
            .split_off(futures.len().saturating_sub(MAX_CONCURRENT_BATCHES))
            .into_iter()
            .collect();

        while let Some(batch_states) = futures_unordered.next().await {
            log::trace!(
                "Received a batch of mirror states ({} states in this batch, {} batches remaining)",
                batch_states.as_ref().map(|m| m.len()).unwrap_or(0),
                futures_unordered.len() + futures.len(),
            );
            program_states.extend(batch_states?);
            if let Some(next_future) = futures.pop() {
                futures_unordered.push(next_future);
            }
        }

        Ok(program_states)
    }
}

async fn collect_mirror_states_batch<N: AlloyNetwork>(
    provider: RootProvider<N>,
    block_id: BlockId,
    mirrors: &[ActorId],
) -> Result<BTreeMap<ActorId, H256>> {
    if mirrors.is_empty() {
        return Ok(BTreeMap::new());
    }

    let calldata = Bytes::from(StateHashCall {}.abi_encode());

    let mut batch = BatchRequest::new(provider.client());
    let mut waiters: Vec<(ActorId, Waiter<Bytes>)> = Vec::with_capacity(mirrors.len());

    for &actor_id in mirrors {
        let mirror = Address::try_from(actor_id)
            .context("Provided actor ID is not a valid mirror address")?;

        let tx = TransactionRequest {
            to: Some(TxKind::Call(AlloyAddress::new(mirror.0))),
            input: TransactionInput::new(calldata.clone()),
            ..Default::default()
        };

        let waiter = batch
            .add_call::<_, Bytes>("eth_call", &(tx, block_id))
            .context("failed to add eth_call to JSON-RPC batch")?;
        waiters.push((actor_id, waiter));
    }

    log::trace!(
        "Sending JSON-RPC batch for eth_call(stateHash) of {} actors at block {block_id}",
        mirrors.len()
    );
    batch
        .send()
        .await
        .context("failed to send JSON-RPC batch")?;

    let mut program_states = BTreeMap::new();
    for (actor_id, waiter) in waiters {
        let bytes = waiter.await.with_context(|| {
            format!("Failed to get state hash for an actor at block {block_id}")
        })?;
        ensure!(
            bytes.len() == 32,
            "Unexpected eth_call(stateHash) response length for actor {actor_id}: expected 32, got {}",
            bytes.len()
        );
        program_states.insert(actor_id, H256::from_slice(bytes.as_ref()));
    }

    Ok(program_states)
}
