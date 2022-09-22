// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use anyhow::Error as AError;
use gp::{api::generated::api::runtime_types::sp_runtime::DispatchError, result::Error as GPError};
use std::{convert::Infallible, io::Error as IOError, result::Result as StdResult};
use subxt::{GenericError, RuntimeError};

pub type Result<T, E = Error> = StdResult<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Anyhow(#[from] AError),
    #[error(transparent)]
    GearProgram(#[from] GPError),
    #[error("An attempt to iter events without subscription")]
    EventsSubscriptionNotFound,
    #[error("Events stopped (unreachable")]
    EventsStopped,
    #[error(transparent)]
    SubxtGeneric(#[from] GenericError<RuntimeError<DispatchError>>),
    #[error(transparent)]
    SubxtInfallible(#[from] GenericError<Infallible>),
    #[error("Expected event wasn't found")]
    EventNotFound,
    #[error(transparent)]
    IO(#[from] IOError),
    #[error("An attempt to upload invalid binary")]
    WrongBinaryExtension,
    #[error("Funds overcame u128::MAX")]
    BalanceOverflow,
    #[error("Block data not found")]
    BlockDataNotFound,
    #[error("Some of extrinsics wasn't processed within the batch")]
    IncompleteBatchResult(usize, usize),
    #[error("Max depth reached, but queried block wasn't found")]
    MaxDepthReached,
    #[error("Event not found in pre-queried events")]
    EventNotFoundInIterator,
}
