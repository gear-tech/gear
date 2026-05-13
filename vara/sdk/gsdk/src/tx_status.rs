// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{TxInBlock, TxStatus};

mod private {
    use crate::TxStatus;

    /// Sealed trait marker.
    pub trait Sealed {}

    impl Sealed for TxStatus {}
}

/// Extension trait for [`TxStatus`] that adds
/// conversion to [`Result<TxSuccess, TxError>`].
pub trait TxStatusExt: private::Sealed {
    fn into_result(self) -> Result<TxSuccess, TxError>;
}

impl TxStatusExt for TxStatus {
    fn into_result(self) -> Result<TxSuccess, TxError> {
        match self {
            TxStatus::Validated => Ok(TxSuccess::Validated),
            TxStatus::Broadcasted => Ok(TxSuccess::Broadcasted),
            TxStatus::NoLongerInBestBlock => Ok(TxSuccess::NoLongerInBestBlock),
            TxStatus::InBestBlock(tx_in_block) => Ok(TxSuccess::InBestBlock(tx_in_block)),
            TxStatus::InFinalizedBlock(tx_in_block) => Ok(TxSuccess::InFinalizedBlock(tx_in_block)),
            TxStatus::Error { message } => Err(TxError::Error { message }),
            TxStatus::Invalid { message } => Err(TxError::Invalid { message }),
            TxStatus::Dropped { message } => Err(TxError::Dropped { message }),
        }
    }
}

/// Transaction Errors
#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("Transaction Error( {message} )")]
    Error { message: String },
    #[error("Transaction Invalid( {message} )")]
    Invalid { message: String },
    #[error("Transaction Dropped( {message} )")]
    Dropped { message: String },
}

/// Successful counterpart of [`TxError`].
///
/// See [`TxStatusExt`].
#[derive(Debug)]
pub enum TxSuccess {
    Validated,
    Broadcasted,
    NoLongerInBestBlock,
    InBestBlock(TxInBlock),
    InFinalizedBlock(TxInBlock),
}

impl From<TxError> for TxStatus {
    fn from(value: TxError) -> Self {
        match value {
            TxError::Error { message } => TxStatus::Error { message },
            TxError::Invalid { message } => TxStatus::Invalid { message },
            TxError::Dropped { message } => TxStatus::Dropped { message },
        }
    }
}

impl From<TxSuccess> for TxStatus {
    fn from(value: TxSuccess) -> Self {
        match value {
            TxSuccess::Validated => TxStatus::Validated,
            TxSuccess::Broadcasted => TxStatus::Broadcasted,
            TxSuccess::NoLongerInBestBlock => TxStatus::NoLongerInBestBlock,
            TxSuccess::InBestBlock(tx_in_block) => TxStatus::InBestBlock(tx_in_block),
            TxSuccess::InFinalizedBlock(tx_in_block) => TxStatus::InFinalizedBlock(tx_in_block),
        }
    }
}
