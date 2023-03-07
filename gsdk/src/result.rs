// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use crate::types::TxStatus;
use sp_core::H256;

/// Transaction Errors
#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("Transaction Retracted( {0} )")]
    Retracted(H256),
    #[error("Transaction Timeout( {0} )")]
    FinalityTimeout(H256),
    #[error("Transaction Usurped( {0} )")]
    Usurped(H256),
    #[error("Transaction Dropped")]
    Dropped,
    #[error("Transaction Invalid")]
    Invalid,
    #[error("Not an error, this will never be reached.")]
    None,
}

impl From<TxStatus> for Error {
    fn from(status: TxStatus) -> Self {
        match status {
            TxStatus::Retracted(h) => TxError::Retracted(h),
            TxStatus::FinalityTimeout(h) => TxError::FinalityTimeout(h),
            TxStatus::Usurped(h) => TxError::Usurped(h),
            TxStatus::Invalid => TxError::Invalid,
            TxStatus::Dropped => TxError::Dropped,
            unreachable => {
                log::info!("Not an error tx status occurred {unreachable:?}");
                TxError::None
            }
        }
        .into()
    }
}

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
    #[error(transparent)]
    Codec(#[from] parity_scale_codec::Error),
    #[error("Code not found {0}")]
    CodeNotFound(String),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error("Unable to get the name of the current executable binary")]
    InvalidExecutable,
    #[error("Password must be provided for logining with json file.")]
    InvalidPassword,
    #[error("Invalid public key")]
    InvalidPublic,
    #[error("Invalid secret key")]
    InvalidSecret,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("The queried storage not found.")]
    StorageNotFound,
    #[error("The queried event not found.")]
    EventNotFound,
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    SubxtPublic(#[from] sp_core::crypto::PublicError),
    #[error(transparent)]
    SubxtMetadata(#[from] subxt::error::MetadataError),
    #[error(transparent)]
    Tx(#[from] TxError),
    #[error(transparent)]
    SubxtRpc(#[from] jsonrpsee::core::Error),
    #[error("Page {0} of Program {1} was not found in the storage.")]
    PageNotFound(u32, String),
    #[error("Program has been terminated.")]
    ProgramTerminated,
    #[error("Invalid rpc URL.")]
    InvalidUrl,
    #[error("Page {0} of Program {1} is invalid.")]
    PageInvalid(u32, String),
}

/// Custom Result
pub type Result<T, E = Error> = std::result::Result<T, E>;
