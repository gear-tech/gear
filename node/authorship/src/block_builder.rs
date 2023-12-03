// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use codec::Encode;
use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use sc_block_builder::{BlockBuilderApi, BuiltBlock, RecordProof};
use sc_client_api::backend;
use sp_api::{ApiExt, ApiRef, Core, ProvideRuntimeApi, TransactionOutcome};
use sp_blockchain::{ApplyExtrinsicFailed, Error};
use sp_core::ExecutionContext;
use sp_runtime::{
    legacy,
    traits::{Block as BlockT, Hash, HashFor, Header as HeaderT, NumberFor, One},
    Digest,
};
use std::ops::DerefMut;

/// Utility for building new (valid) blocks from a stream of extrinsics.
pub struct BlockBuilder<'a, Block: BlockT, A: ProvideRuntimeApi<Block>, B> {
    extrinsics: Vec<Block::Extrinsic>,
    api: ApiRef<'a, A::Api>,
    version: u32,
    parent_hash: Block::Hash,
    backend: &'a B,
    /// The estimated size of the block header.
    estimated_header_size: usize,
}

impl<'a, Block, A, B> BlockBuilder<'a, Block, A, B>
where
    Block: BlockT,
    A: ProvideRuntimeApi<Block> + 'a,
    A::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
    B: backend::Backend<Block>,
{
    /// Create a new instance of builder based on the given `parent_hash` and `parent_number`.
    ///
    /// While proof recording is enabled, all accessed trie nodes are saved.
    /// These recorded trie nodes can be used by a third party to prove the
    /// output of this block builder without having access to the full storage.
    pub fn new(
        api: &'a A,
        parent_hash: Block::Hash,
        parent_number: NumberFor<Block>,
        record_proof: RecordProof,
        inherent_digests: Digest,
        backend: &'a B,
    ) -> Result<Self, Error> {
        let header = <<Block as BlockT>::Header as HeaderT>::new(
            parent_number + One::one(),
            Default::default(),
            Default::default(),
            parent_hash,
            inherent_digests,
        );

        let estimated_header_size = header.encoded_size();

        let mut api = api.runtime_api();

        if record_proof.yes() {
            api.record_proof();
        }

        api.initialize_block_with_context(
            parent_hash,
            ExecutionContext::BlockConstruction,
            &header,
        )?;

        let version = api
            .api_version::<dyn BlockBuilderApi<Block>>(parent_hash)?
            .ok_or_else(|| Error::VersionInvalid("BlockBuilderApi".to_string()))?;

        Ok(Self {
            parent_hash,
            extrinsics: Vec::new(),
            api,
            version,
            backend,
            estimated_header_size,
        })
    }

    /// Push onto the block's list of extrinsics.
    ///
    /// This will ensure the extrinsic can be validly executed (by executing it).
    pub fn push(&mut self, xt: <Block as BlockT>::Extrinsic) -> Result<(), Error> {
        let parent_hash = self.parent_hash;
        let extrinsics = &mut self.extrinsics;
        let version = self.version;

        self.api.execute_in_transaction(|api| {
            let res = if version < 6 {
                #[allow(deprecated)]
                api.apply_extrinsic_before_version_6_with_context(
                    parent_hash,
                    ExecutionContext::BlockConstruction,
                    xt.clone(),
                )
                .map(legacy::byte_sized_error::convert_to_latest)
            } else {
                api.apply_extrinsic_with_context(
                    parent_hash,
                    ExecutionContext::BlockConstruction,
                    xt.clone(),
                )
            };

            match res {
                Ok(Ok(_)) => {
                    extrinsics.push(xt);
                    TransactionOutcome::Commit(Ok(()))
                }
                Ok(Err(tx_validity)) => TransactionOutcome::Rollback(Err(
                    ApplyExtrinsicFailed::Validity(tx_validity).into(),
                )),
                Err(e) => TransactionOutcome::Rollback(Err(Error::from(e))),
            }
        })
    }

    /// Try to fetch the `pseudo-inherent` via the RuntimeAPI call and, if successful,
    /// push it at the end of the block's list of extrinsics.
    ///
    /// This will ensure the extrinsic can be validly executed (by executing it).
    pub fn push_final(&mut self, max_gas: Option<u64>) -> Result<(), Error> {
        let parent_hash = self.parent_hash;
        let extrinsics = &mut self.extrinsics;
        let version = self.version;

        self.api.execute_in_transaction(|api| {
            let block_hash = self.parent_hash;
            let xt = match TransactionOutcome::Rollback(api.gear_run_extrinsic_with_context(
                block_hash,
                ExecutionContext::BlockConstruction,
                max_gas,
            ))
            .into_inner()
            .map_err(|e| Error::Application(Box::new(e)))
            {
                Ok(xt) => xt,
                Err(e) => return TransactionOutcome::Rollback(Err(e)),
            };

            let res = if version < 6 {
                #[allow(deprecated)]
                api.apply_extrinsic_before_version_6_with_context(
                    parent_hash,
                    ExecutionContext::BlockConstruction,
                    xt.clone(),
                )
                .map(legacy::byte_sized_error::convert_to_latest)
            } else {
                api.apply_extrinsic_with_context(
                    parent_hash,
                    ExecutionContext::BlockConstruction,
                    xt.clone(),
                )
            };

            match res {
                Ok(Ok(_)) => {
                    extrinsics.push(xt);
                    TransactionOutcome::Commit(Ok(()))
                }
                Ok(Err(tx_validity)) => TransactionOutcome::Rollback(Err(
                    ApplyExtrinsicFailed::Validity(tx_validity).into(),
                )),
                Err(e) => TransactionOutcome::Rollback(Err(Error::from(e))),
            }
        })
    }

    /// Consume the builder to build a valid `Block` containing all pushed extrinsics.
    ///
    /// Returns the build `Block`, the changes to the storage and an optional `StorageProof`
    /// supplied by `self.api`, combined as [`BuiltBlock`].
    /// The storage proof will be `Some(_)` when proof recording was enabled.
    pub fn build(mut self) -> Result<BuiltBlock<Block, backend::StateBackendFor<B, Block>>, Error> {
        let header = self
            .api
            .finalize_block_with_context(self.parent_hash, ExecutionContext::BlockConstruction)?;

        debug_assert_eq!(
            header.extrinsics_root().clone(),
            HashFor::<Block>::ordered_trie_root(
                self.extrinsics.iter().map(Encode::encode).collect(),
                sp_runtime::StateVersion::V0,
            ),
        );

        let proof = self.api.extract_proof();

        let state = self.backend.state_at(self.parent_hash)?;

        let storage_changes = self
            .api
            .into_storage_changes(&state, self.parent_hash)
            .map_err(sp_blockchain::Error::StorageChanges)?;

        Ok(BuiltBlock {
            block: <Block as BlockT>::new(header, self.extrinsics),
            storage_changes,
            proof,
        })
    }

    /// Create the inherents for the block.
    ///
    /// Returns the inherents created by the runtime or an error if something failed.
    pub fn create_inherents(
        &mut self,
        inherent_data: sp_inherents::InherentData,
    ) -> Result<Vec<Block::Extrinsic>, Error> {
        let parent_hash = self.parent_hash;
        self.api
            .execute_in_transaction(move |api| {
                // `create_inherents` should not change any state, to ensure this we always rollback
                // the transaction.
                TransactionOutcome::Rollback(api.inherent_extrinsics_with_context(
                    parent_hash,
                    ExecutionContext::BlockConstruction,
                    inherent_data,
                ))
            })
            .map_err(|e| Error::Application(Box::new(e)))
    }

    /// Estimate the size of the block in the current state.
    ///
    /// If `include_proof` is `true`, the estimated size of the storage proof will be added
    /// to the estimation.
    pub fn estimate_block_size(&self, include_proof: bool) -> usize {
        let size = self.estimated_header_size + self.extrinsics.encoded_size();

        if include_proof {
            size + self
                .api
                .proof_recorder()
                .map(|pr| pr.estimate_encoded_size())
                .unwrap_or(0)
        } else {
            size
        }
    }

    #[cfg(test)]
    pub fn extrinsics(&self) -> &[Block::Extrinsic] {
        &self.extrinsics[..]
    }

    #[cfg(test)]
    pub fn into_storage_changes(
        self,
    ) -> Result<sp_api::StorageChanges<backend::StateBackendFor<B, Block>, Block>, Error> {
        let state = self.backend.state_at(self.parent_hash)?;

        let storage_changes = self
            .api
            .into_storage_changes(&state, self.parent_hash)
            .map_err(sp_blockchain::Error::StorageChanges)?;

        Ok(storage_changes)
    }

    /// Break a builder instance into its parts.
    #[allow(clippy::type_complexity)]
    pub fn deconstruct(
        self,
    ) -> (
        Vec<Block::Extrinsic>,
        ApiRef<'a, A::Api>,
        u32,
        Block::Hash,
        &'a B,
        usize,
    ) {
        (
            self.extrinsics,
            self.api,
            self.version,
            self.parent_hash,
            self.backend,
            self.estimated_header_size,
        )
    }

    /// Restore a builder instance from its parts.
    pub fn from_parts(
        extrinsics: Vec<Block::Extrinsic>,
        api: ApiRef<'a, A::Api>,
        version: u32,
        parent_hash: Block::Hash,
        backend: &'a B,
        estimated_header_size: usize,
    ) -> Self {
        Self {
            extrinsics,
            api,
            version,
            parent_hash,
            backend,
            estimated_header_size,
        }
    }

    /// Replace the runtime api with the given one.
    pub fn set_api(&mut self, api: &mut A::Api) {
        std::mem::swap(self.api.deref_mut(), api);
    }

    /// Replace the extrinsics with the given ones.
    pub fn set_extrinsics(&mut self, extrinsics: Vec<Block::Extrinsic>) {
        self.extrinsics = extrinsics;
    }
}

impl<'a, Block, A, B> Clone for BlockBuilder<'a, Block, A, B>
where
    Block: BlockT,
    A: ProvideRuntimeApi<Block> + 'a,
    A::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>
        + Clone,
    B: backend::Backend<Block>,
{
    fn clone(&self) -> Self {
        Self {
            extrinsics: self.extrinsics.clone(),
            api: self.api.clone().into(),
            version: self.version,
            parent_hash: self.parent_hash,
            backend: <&B>::clone(&self.backend),
            estimated_header_size: self.estimated_header_size,
        }
    }
}
