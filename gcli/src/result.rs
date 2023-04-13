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

//! Custom result

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    GSdk(#[from] gsdk::result::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("Invalid node key")]
    BadNodeKey,
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
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
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Logger(#[from] log::SetLoggerError),
    #[error("No available account was found in keystore, please run `gear login` first.")]
    Logout,
    #[error("{0}")]
    Nacl(String),
    #[error("{0}")]
    Schnorrkel(schnorrkel::SignatureError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    SubxtPublic(#[from] gsdk::ext::sp_core::crypto::PublicError),
    #[error("Type {0} not found in registry")]
    TypeNotFound(String),
    #[error(transparent)]
    Codec(#[from] scale_info::scale::Error),
    #[error("{0:?}")]
    Code(gear_core::code::CodeError),
    #[error("Invalid wasm file")]
    InvalidWasm,
    #[error("Wasm execution error {0}")]
    WasmExecution(String),
}

impl From<nacl::Error> for Error {
    fn from(err: nacl::Error) -> Self {
        Self::Nacl(err.message)
    }
}

impl From<schnorrkel::SignatureError> for Error {
    fn from(err: schnorrkel::SignatureError) -> Self {
        Self::Schnorrkel(err)
    }
}

impl From<gear_core::code::CodeError> for Error {
    fn from(err: gear_core::code::CodeError) -> Self {
        Self::Code(err)
    }
}

/// Custom result
pub type Result<T, E = Error> = std::result::Result<T, E>;
