// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Implementation of the `chainHead_storage` method.

use std::{collections::VecDeque, marker::PhantomData, sync::Arc};

use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sc_utils::mpsc::TracingUnboundedSender;
use sp_runtime::traits::Block as BlockT;

use crate::{
    chain_head::{
        chain_head::spawn_blocking,
        event::{OperationError, OperationId, OperationStorageItems},
        subscription::BlockGuard,
        FollowEvent,
    },
    common::{
        events::{StorageQuery, StorageQueryType},
        storage::{IterQueryType, QueryIter, QueryIterResult, Storage},
    },
    SubscriptionTaskExecutor,
};

/// Generates the events of the `chainHead_storage` method.
pub struct ChainHeadStorage<Client, Block, BE> {
    /// Storage client.
    client: Storage<Client, Block, BE>,
    /// Queue of operations that may require pagination.
    iter_operations: VecDeque<QueryIter>,
    /// The maximum number of items reported by the `chainHead_storage` before
    /// pagination is required.
    operation_max_storage_items: usize,
    _phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE> {
    /// Constructs a new [`ChainHeadStorage`].
    pub fn new(client: Arc<Client>, operation_max_storage_items: usize) -> Self {
        Self {
            client: Storage::new(client),
            iter_operations: VecDeque::new(),
            operation_max_storage_items,
            _phandom: PhantomData,
        }
    }
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE>
where
    Block: BlockT + 'static,
    BE: Backend<Block> + 'static,
    Client: StorageProvider<Block, BE> + Send + Sync + 'static,
{
    /// Generate storage events, running finite database batches on the blocking executor.
    ///
    /// Each batch owns the block guard until its result is returned, so cancelling this
    /// driver cannot unpin a block while a queued or running batch still accesses it.
    pub async fn generate_events(
        mut self,
        mut block_guard: BlockGuard<Block, BE>,
        hash: Block::Hash,
        items: Vec<StorageQuery<StorageKey>>,
        child_key: Option<ChildInfo>,
        executor: SubscriptionTaskExecutor,
    ) {
        if block_guard.operation().was_stopped() {
            return;
        }

        let Ok(Some((mut storage, mut block_guard, mut child_key, result))) =
            spawn_blocking(&executor, move || {
                let mut storage_results = Vec::with_capacity(items.len());
                for item in items {
                    if block_guard.operation().was_stopped() {
                        return None;
                    }

                    let result = match item.query_type {
                        StorageQueryType::Value => {
                            self.client.query_value(hash, &item.key, child_key.as_ref())
                        }
                        StorageQueryType::Hash => {
                            self.client.query_hash(hash, &item.key, child_key.as_ref())
                        }
                        StorageQueryType::ClosestDescendantMerkleValue => self
                            .client
                            .query_merkle_value(hash, &item.key, child_key.as_ref()),
                        StorageQueryType::DescendantsValues => {
                            self.iter_operations.push_back(QueryIter {
                                query_key: item.key,
                                ty: IterQueryType::Value,
                                pagination_start_key: None,
                            });
                            continue;
                        }
                        StorageQueryType::DescendantsHashes => {
                            self.iter_operations.push_back(QueryIter {
                                query_key: item.key,
                                ty: IterQueryType::Hash,
                                pagination_start_key: None,
                            });
                            continue;
                        }
                    };

                    match result {
                        Ok(Some(value)) => storage_results.push(value),
                        Ok(None) => {}
                        Err(error) => return Some((self, block_guard, child_key, Err(error))),
                    }
                }

                Some((self, block_guard, child_key, Ok(storage_results)))
            })
            .await
        else {
            return;
        };

        let sender = block_guard.response_sender();
        let operation = block_guard.operation();
        if operation.was_stopped() {
            return;
        }
        let storage_results = match result {
            Ok(results) => results,
            Err(error) => {
                send_error::<Block>(&sender, operation.operation_id(), error);
                return;
            }
        };
        if !storage_results.is_empty() {
            let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageItems(
                OperationStorageItems {
                    operation_id: operation.operation_id(),
                    items: storage_results,
                },
            ));
        }

        while let Some(query) = storage.iter_operations.pop_front() {
            if block_guard.operation().was_stopped() {
                return;
            }

            let Ok(Some((next_storage, next_guard, next_child_key, result))) =
                spawn_blocking(&executor, move || {
                    if block_guard.operation().was_stopped() {
                        return None;
                    }
                    let result = storage.client.query_iter_pagination(
                        query,
                        hash,
                        child_key.as_ref(),
                        storage.operation_max_storage_items,
                    );
                    Some((storage, block_guard, child_key, result))
                })
                .await
            else {
                return;
            };
            storage = next_storage;
            block_guard = next_guard;
            child_key = next_child_key;

            let operation = block_guard.operation();
            if operation.was_stopped() {
                return;
            }
            let (events, maybe_next_query) = match result {
                QueryIterResult::Ok(result) => result,
                QueryIterResult::Err(error) => {
                    send_error::<Block>(&sender, operation.operation_id(), error);
                    return;
                }
            };

            if !events.is_empty() {
                let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageItems(
                    OperationStorageItems {
                        operation_id: operation.operation_id(),
                        items: events,
                    },
                ));
            }

            if let Some(next_query) = maybe_next_query {
                if operation.was_stopped() {
                    return;
                }
                let _ = sender.unbounded_send(
                    FollowEvent::<Block::Hash>::OperationWaitingForContinue(OperationId {
                        operation_id: operation.operation_id(),
                    }),
                );

                // Waiting for the client must never occupy a blocking worker.
                operation.wait_for_continue().await;
                if operation.was_stopped() {
                    return;
                }

                // Give a chance for the other items to advance next time.
                storage.iter_operations.push_back(next_query);
            }
        }

        let operation = block_guard.operation();
        if operation.was_stopped() {
            return;
        }
        let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageDone(
            OperationId {
                operation_id: operation.operation_id(),
            },
        ));
    }
}

/// Build and send the opaque error back to the `chainHead_follow` method.
fn send_error<Block: BlockT>(
    sender: &TracingUnboundedSender<FollowEvent<Block::Hash>>,
    operation_id: String,
    error: String,
) {
    let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationError(OperationError {
        operation_id,
        error,
    }));
}
