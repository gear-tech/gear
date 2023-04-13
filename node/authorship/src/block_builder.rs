// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use sc_block_builder::{BlockBuilder, BlockBuilderApi, BuiltBlock};
use sc_client_api::backend;
use sp_api::{ApiExt, ApiRef, ProvideRuntimeApi, TransactionOutcome};
use sp_blockchain::Error;
use sp_core::ExecutionContext;
use sp_runtime::traits::Block as BlockT;

pub struct BlockBuilderExt<'a, Block: BlockT, A: ProvideRuntimeApi<Block>, B> {
    block_builder: BlockBuilder<'a, Block, A, B>,
    api: ApiRef<'a, A::Api>,
    parent_hash: Block::Hash,
}

impl<'a, Block, A, B> BlockBuilderExt<'a, Block, A, B>
where
    Block: BlockT,
    A: ProvideRuntimeApi<Block> + 'a,
    A::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
    B: backend::Backend<Block>,
{
    /// Creating a block builder by wrapping an sc_block_builder::BlockBuilder object
    pub fn new(
        block_builder: BlockBuilder<'a, Block, A, B>,
        api: ApiRef<'a, A::Api>,
        parent_hash: Block::Hash,
    ) -> Self {
        Self {
            block_builder,
            api,
            parent_hash,
        }
    }

    /// Push onto the block's list of extrinsics.
    pub fn push(&mut self, xt: <Block as BlockT>::Extrinsic) -> Result<(), Error> {
        self.block_builder.push(xt)
    }

    /// Consume the builder to build a valid `Block` containing all pushed extrinsics.
    pub fn build(self) -> Result<BuiltBlock<Block, backend::StateBackendFor<B, Block>>, Error> {
        self.block_builder.build()
    }

    /// Create the inherents for the block.
    pub fn create_inherents(
        &mut self,
        inherent_data: sp_inherents::InherentData,
    ) -> Result<Vec<Block::Extrinsic>, Error> {
        self.block_builder.create_inherents(inherent_data)
    }

    /// Estimate the size of the block in the current state.
    pub fn estimate_block_size(&self, include_proof: bool) -> usize {
        self.block_builder.estimate_block_size(include_proof)
    }

    pub fn create_terminal_extrinsic(&mut self) -> Result<Block::Extrinsic, Error> {
        let block_hash = self.parent_hash;
        self.api
            .execute_in_transaction(move |api| {
                TransactionOutcome::Rollback(api.gear_run_extrinsic_with_context(
                    block_hash,
                    ExecutionContext::BlockConstruction,
                ))
            })
            .map_err(|e| Error::Application(Box::new(e)))
    }
}
