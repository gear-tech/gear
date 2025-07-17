// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use parity_scale_codec::Encode;
use sc_block_builder::{BlockBuilderApi, BuiltBlock};
use sp_api::{ApiExt, ApiRef, CallApiAt, Core, ProvideRuntimeApi, TransactionOutcome};
use sp_blockchain::{ApplyExtrinsicFailed, Error, HeaderBackend};
use sp_runtime::{
    Digest, ExtrinsicInclusionMode, legacy,
    traits::{Block as BlockT, Hash, HashingFor, Header as HeaderT, NumberFor, One},
};
use std::{marker::PhantomData, ops::DerefMut};

/// A builder for creating an instance of [`BlockBuilder`].
pub struct BlockBuilderBuilder<'a, B, C> {
    call_api_at: &'a C,
    _phantom: PhantomData<B>,
}

impl<'a, B, C> BlockBuilderBuilder<'a, B, C>
where
    B: BlockT,
{
    /// Create a new instance of the builder.
    ///
    /// `call_api_at`: Something that implements [`CallApiAt`].
    pub fn new(call_api_at: &'a C) -> Self {
        Self {
            call_api_at,
            _phantom: PhantomData,
        }
    }

    /// Specify the parent block to build on top of.
    pub fn on_parent_block(self, parent_block: B::Hash) -> BlockBuilderBuilderStage1<'a, B, C> {
        BlockBuilderBuilderStage1 {
            call_api_at: self.call_api_at,
            parent_block,
        }
    }
}

/// The second stage of the [`BlockBuilderBuilder`].
///
/// This type can not be instantiated directly. To get an instance of it
/// [`BlockBuilderBuilder::new`] needs to be used.
pub struct BlockBuilderBuilderStage1<'a, B: BlockT, C> {
    call_api_at: &'a C,
    parent_block: B::Hash,
}

impl<'a, B, C> BlockBuilderBuilderStage1<'a, B, C>
where
    B: BlockT,
{
    /// Fetch the parent block number from the given `header_backend`.
    ///
    /// The parent block number is used to initialize the block number of the new block.
    ///
    /// Returns an error if the parent block specified in
    /// [`on_parent_block`](BlockBuilderBuilder::on_parent_block) does not exist.
    #[allow(unused)]
    pub fn fetch_parent_block_number<H: HeaderBackend<B>>(
        self,
        header_backend: &H,
    ) -> Result<BlockBuilderBuilderStage2<'a, B, C>, Error> {
        let parent_number = header_backend.number(self.parent_block)?.ok_or_else(|| {
            Error::Backend(format!(
                "Could not fetch block number for block: {:?}",
                self.parent_block
            ))
        })?;

        Ok(BlockBuilderBuilderStage2 {
            call_api_at: self.call_api_at,
            enable_proof_recording: false,
            inherent_digests: Default::default(),
            parent_block: self.parent_block,
            parent_number,
        })
    }

    /// Provide the block number for the parent block directly.
    ///
    /// The parent block is specified in [`on_parent_block`](BlockBuilderBuilder::on_parent_block).
    /// The parent block number is used to initialize the block number of the new block.
    pub fn with_parent_block_number(
        self,
        parent_number: NumberFor<B>,
    ) -> BlockBuilderBuilderStage2<'a, B, C> {
        BlockBuilderBuilderStage2 {
            call_api_at: self.call_api_at,
            enable_proof_recording: false,
            inherent_digests: Default::default(),
            parent_block: self.parent_block,
            parent_number,
        }
    }
}

/// The second stage of the [`BlockBuilderBuilder`].
///
/// This type can not be instantiated directly. To get an instance of it
/// [`BlockBuilderBuilder::new`] needs to be used.
pub struct BlockBuilderBuilderStage2<'a, B: BlockT, C> {
    call_api_at: &'a C,
    enable_proof_recording: bool,
    inherent_digests: Digest,
    parent_block: B::Hash,
    parent_number: NumberFor<B>,
}

impl<'a, B: BlockT, C> BlockBuilderBuilderStage2<'a, B, C> {
    /// Enable proof recording for the block builder.
    #[allow(unused)]
    pub fn enable_proof_recording(mut self) -> Self {
        self.enable_proof_recording = true;
        self
    }

    /// Enable/disable proof recording for the block builder.
    pub fn with_proof_recording(mut self, enable: bool) -> Self {
        self.enable_proof_recording = enable;
        self
    }

    /// Build the block with the given inherent digests.
    pub fn with_inherent_digests(mut self, inherent_digests: Digest) -> Self {
        self.inherent_digests = inherent_digests;
        self
    }

    /// Create the instance of the [`BlockBuilder`].
    pub fn build(self) -> Result<BlockBuilder<'a, B, C>, Error>
    where
        C: CallApiAt<B> + ProvideRuntimeApi<B>,
        C::Api: BlockBuilderApi<B> + GearRuntimeApi<B>,
    {
        BlockBuilder::new(
            self.call_api_at,
            self.parent_block,
            self.parent_number,
            self.enable_proof_recording,
            self.inherent_digests,
        )
    }
}

/// Utility for building new (valid) blocks from a stream of extrinsics.
pub struct BlockBuilder<'a, Block: BlockT, C: ProvideRuntimeApi<Block> + 'a> {
    extrinsics: Vec<Block::Extrinsic>,
    api: ApiRef<'a, C::Api>,
    call_api_at: &'a C,
    version: u32,
    parent_hash: Block::Hash,
    /// The estimated size of the block header.
    estimated_header_size: usize,
    extrinsic_inclusion_mode: ExtrinsicInclusionMode,
}

impl<'a, Block, C> BlockBuilder<'a, Block, C>
where
    Block: BlockT,
    C: CallApiAt<Block> + ProvideRuntimeApi<Block> + 'a,
    C::Api: BlockBuilderApi<Block> + GearRuntimeApi<Block>,
{
    /// Create a new instance of builder based on the given `parent_hash` and `parent_number`.
    ///
    /// While proof recording is enabled, all accessed trie nodes are saved.
    /// These recorded trie nodes can be used by a third party to prove the
    /// output of this block builder without having access to the full storage.
    fn new(
        call_api_at: &'a C,
        parent_hash: Block::Hash,
        parent_number: NumberFor<Block>,
        record_proof: bool,
        inherent_digests: Digest,
    ) -> Result<Self, Error> {
        let header = <<Block as BlockT>::Header as HeaderT>::new(
            parent_number + One::one(),
            Default::default(),
            Default::default(),
            parent_hash,
            inherent_digests,
        );

        let estimated_header_size = header.encoded_size();

        let mut api = call_api_at.runtime_api();

        if record_proof {
            api.record_proof();
        }

        let core_version = api
            .api_version::<dyn Core<Block>>(parent_hash)?
            .ok_or_else(|| Error::VersionInvalid("Core".to_string()))?;

        let extrinsic_inclusion_mode = if core_version >= 5 {
            api.initialize_block(parent_hash, &header)?
        } else {
            #[allow(deprecated)]
            api.initialize_block_before_version_5(parent_hash, &header)?;
            ExtrinsicInclusionMode::AllExtrinsics
        };

        let bb_version = api
            .api_version::<dyn BlockBuilderApi<Block>>(parent_hash)?
            .ok_or_else(|| Error::VersionInvalid("BlockBuilderApi".to_string()))?;

        Ok(Self {
            parent_hash,
            extrinsics: Vec::new(),
            api,
            version: bb_version,
            estimated_header_size,
            call_api_at,
            extrinsic_inclusion_mode,
        })
    }

    /// The extrinsic inclusion mode of the runtime for this block.
    #[allow(unused)]
    pub fn extrinsic_inclusion_mode(&self) -> ExtrinsicInclusionMode {
        self.extrinsic_inclusion_mode
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
                api.apply_extrinsic_before_version_6(parent_hash, xt.clone())
                    .map(legacy::byte_sized_error::convert_to_latest)
            } else {
                api.apply_extrinsic(parent_hash, xt.clone())
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
            let xt = match TransactionOutcome::Rollback(api.gear_run_extrinsic(block_hash, max_gas))
                .into_inner()
                .map_err(|e| Error::Application(Box::new(e)))
            {
                Ok(xt) => xt,
                Err(e) => return TransactionOutcome::Rollback(Err(e)),
            };

            let res = if version < 6 {
                #[allow(deprecated)]
                api.apply_extrinsic_before_version_6(parent_hash, xt.clone())
                    .map(legacy::byte_sized_error::convert_to_latest)
            } else {
                api.apply_extrinsic(parent_hash, xt.clone())
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
    pub fn build(mut self) -> Result<BuiltBlock<Block>, Error> {
        let header = self.api.finalize_block(self.parent_hash)?;

        debug_assert_eq!(
            header.extrinsics_root().clone(),
            HashingFor::<Block>::ordered_trie_root(
                self.extrinsics.iter().map(Encode::encode).collect(),
                sp_runtime::StateVersion::V0,
            ),
        );

        let proof = self.api.extract_proof();

        let state = self.call_api_at.state_at(self.parent_hash)?;

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
                TransactionOutcome::Rollback(api.inherent_extrinsics(parent_hash, inherent_data))
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
    pub fn into_storage_changes(self) -> Result<sp_api::StorageChanges<Block>, Error> {
        let state = self.call_api_at.state_at(self.parent_hash)?;

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
        ApiRef<'a, C::Api>,
        &'a C,
        u32,
        Block::Hash,
        usize,
    ) {
        (
            self.extrinsics,
            self.api,
            self.call_api_at,
            self.version,
            self.parent_hash,
            self.estimated_header_size,
        )
    }

    /// Restore a builder instance from its parts.
    pub fn from_parts(
        extrinsics: Vec<Block::Extrinsic>,
        api: ApiRef<'a, C::Api>,
        call_api_at: &'a C,
        version: u32,
        parent_hash: Block::Hash,
        estimated_header_size: usize,
    ) -> Self {
        Self {
            extrinsics,
            api,
            call_api_at,
            version,
            parent_hash,
            estimated_header_size,
            extrinsic_inclusion_mode: ExtrinsicInclusionMode::AllExtrinsics,
        }
    }

    /// Replace the runtime api with the given one.
    pub fn set_api(&mut self, api: &mut C::Api) {
        std::mem::swap(self.api.deref_mut(), api);
    }

    /// Replace the extrinsics with the given ones.
    pub fn set_extrinsics(&mut self, extrinsics: Vec<Block::Extrinsic>) {
        self.extrinsics = extrinsics;
    }
}

impl<'a, Block, C> Clone for BlockBuilder<'a, Block, C>
where
    Block: BlockT,
    C: CallApiAt<Block> + ProvideRuntimeApi<Block> + 'a,
    C::Api: BlockBuilderApi<Block> + GearRuntimeApi<Block> + Clone,
{
    fn clone(&self) -> Self {
        Self {
            extrinsics: self.extrinsics.clone(),
            api: self.api.clone().into(),
            call_api_at: self.call_api_at,
            version: self.version,
            parent_hash: self.parent_hash,
            estimated_header_size: self.estimated_header_size,
            extrinsic_inclusion_mode: self.extrinsic_inclusion_mode,
        }
    }
}
