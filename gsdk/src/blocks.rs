// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! This module provides useful functions for working with block stream.

use std::pin::pin;

use futures::prelude::*;

use subxt::blocks::Block;

use crate::{Error, GearConfig, Result};

/// Checks whether the blocks are progressing.
pub async fn are_progressing<E>(
    blocks: impl Stream<Item = Result<Block<GearConfig, subxt::OnlineClient<GearConfig>>, E>>,
) -> Result<bool>
where
    Error: From<E>,
{
    let mut blocks = pin!(blocks);

    let current_block = blocks
        .next()
        .await
        .transpose()?
        .ok_or(Error::SubscriptionDied)?;
    let next_block = blocks
        .next()
        .await
        .transpose()?
        .ok_or(Error::SubscriptionDied)?;

    Ok(current_block.number() != next_block.number())
}
