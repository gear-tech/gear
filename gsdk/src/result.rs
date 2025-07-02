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
use crate::TxStatus;

/// Transaction Errors
#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("Transaction Error( {0} )")]
    Error(String),
    #[error("Transaction Invalid( {0} )")]
    Invalid(String),
    #[error("Transaction Dropped( {0} )")]
    Dropped(String),
    #[error("Not an error, this will never be reached.")]
    None,
}

impl From<TxStatus> for Error {
    fn from(status: TxStatus) -> Self {
        match status {
            TxStatus::Error { message } => TxError::Error(message),
            TxStatus::Invalid { message } => TxError::Invalid(message),
            TxStatus::Dropped { message } => TxError::Dropped(message),
            unreachable => {
                log::info!("Not an error tx status occurred {unreachable:?}");
                TxError::None
            }
        }
        .into()
    }
}

/// Errors
#[derive(Debug, thiserror::Error, derive_more::Unwrap)]
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
    #[error("Invalid RPC params")]
    InvalidRpcParams,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("The queried storage not found.")]
    StorageNotFound,
    #[error("The queried event not found.")]
    EventNotFound,
    #[error(transparent)]
    Subxt(#[from] Box<subxt::Error>),
    #[error(transparent)]
    SubxtCore(#[from] Box<subxt::ext::subxt_core::Error>),
    #[error(transparent)]
    SubxtPublic(#[from] sp_core::crypto::PublicError),
    #[error(transparent)]
    SubxtMetadata(#[from] subxt::error::MetadataError),
    #[error(transparent)]
    ScaleValueEncode(#[from] Box<scale_value::scale::EncodeError>),
    #[error(transparent)]
    Tx(#[from] TxError),
    #[error(transparent)]
    SubxtRpc(#[from] jsonrpsee::core::ClientError),
    #[error("Page {0} of Program {1} was not found in the storage.")]
    PageNotFound(u32, String),
    #[error("Program has been terminated.")]
    ProgramTerminated,
    #[error("Invalid rpc URL.")]
    InvalidUrl,
    #[error("Page {0} of Program {1} is invalid.")]
    PageInvalid(u32, String),
}

impl From<subxt::Error> for Error {
    fn from(value: subxt::Error) -> Self {
        Self::Subxt(Box::new(value))
    }
}

impl From<subxt::ext::subxt_core::Error> for Error {
    fn from(value: subxt::ext::subxt_core::Error) -> Self {
        Self::SubxtCore(Box::new(value))
    }
}

impl From<scale_value::scale::EncodeError> for Error {
    fn from(value: scale_value::scale::EncodeError) -> Self {
        Self::ScaleValueEncode(Box::new(value))
    }
}

/// Custom Result
pub type Result<T, E = Error> = std::result::Result<T, E>;
