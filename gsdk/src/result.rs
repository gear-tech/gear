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

//! GSdk Results

pub use crate::tx_status::{TxError, TxStatusExt, TxSuccess};

use gear_core::ids::ActorId;
use subxt::ext::{scale_encode, subxt_rpcs};

#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display)]
#[display("Page {index} of Program {program}")]
pub struct FailedPage {
    pub index: u32,
    pub program: ActorId,
}

impl FailedPage {
    pub fn new(index: u32, program: ActorId) -> Self {
        Self { index, program }
    }

    pub fn invalid(self) -> Error {
        Error::InvalidPage(self)
    }

    pub fn not_found(self) -> Error {
        Error::PageNotFound(self)
    }
}

#[derive(Debug, thiserror::Error, derive_more::Unwrap)]
pub enum Error {
    #[error("the queried event not found")]
    EventNotFound,

    #[error("the queried storage entry not found")]
    StorageEntryNotFound,

    #[error("subscription has been died")]
    SubscriptionDied,

    #[error("program has been terminated")]
    ProgramTerminated,

    #[error("{0} is invalid")]
    InvalidPage(FailedPage),

    #[error("{0} was not found in the storage")]
    PageNotFound(FailedPage),

    #[error(transparent)]
    Tx(#[from] TxError),

    #[error("failed to parse URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error(transparent)]
    ClientError(#[from] jsonrpsee::core::ClientError),

    #[error(transparent)]
    Subxt(#[from] Box<subxt::Error>),

    #[error(transparent)]
    Codec(#[from] subxt::ext::codec::Error),

    #[error(transparent)]
    SubxtRpc(#[from] subxt_rpcs::Error),

    #[error(transparent)]
    SecretString(#[from] sp_core::crypto::SecretStringError),

    #[error(transparent)]
    ScaleEncode(#[from] scale_encode::Error),

    #[error(transparent)]
    Crypto(#[from] sp_core::crypto::PublicError),

    #[error(transparent)]
    Metadata(#[from] subxt::error::MetadataError),
}

impl From<subxt::Error> for Error {
    fn from(error: subxt::Error) -> Self {
        Self::Subxt(Box::new(error))
    }
}

/// Custom Result
pub type Result<T, E = Error> = std::result::Result<T, E>;
