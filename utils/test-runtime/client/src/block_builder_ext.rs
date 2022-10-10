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

//! Block Builder extensions for tests.

use sc_client_api::backend;
use sp_api::{ApiExt, ProvideRuntimeApi};

use sc_block_builder::BlockBuilderApi;

// Extension trait for test block builder.
pub trait BlockBuilderExt {
    // Add submit extrinsic to the block.
    fn push_submit(&mut self, message: test_runtime::Message) -> Result<(), sp_blockchain::Error>;
    // Add storage change extrinsic to the block.
    fn push_storage_change(
        &mut self,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> Result<(), sp_blockchain::Error>;
}

impl<'a, A, B> BlockBuilderExt for sc_block_builder::BlockBuilder<'a, test_runtime::Block, A, B>
where
    A: ProvideRuntimeApi<test_runtime::Block> + 'a,
    A::Api: BlockBuilderApi<test_runtime::Block>
        + ApiExt<test_runtime::Block, StateBackend = backend::StateBackendFor<B, test_runtime::Block>>,
    B: backend::Backend<test_runtime::Block>,
{
    fn push_submit(&mut self, message: test_runtime::Message) -> Result<(), sp_blockchain::Error> {
        self.push(message.into_signed_tx())
    }

    fn push_storage_change(
        &mut self,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> Result<(), sp_blockchain::Error> {
        self.push(test_runtime::Extrinsic::StorageChange(key, value))
    }
}
