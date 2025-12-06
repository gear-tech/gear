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

//! Custom result

/// Errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    GSdk(#[from] gsdk::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Logger(#[from] log::SetLoggerError),
    #[error("No available account was found in keystore, please run `gear login` first.")]
    Logout,
    #[error(transparent)]
    SubxtPublic(#[from] gsdk::ext::sp_core::crypto::PublicError),
    #[error("Type {0} not found in registry")]
    TypeNotFound(String),
    #[error(transparent)]
    Codec(#[from] scale_info::scale::Error),
    #[error("{0:?}")]
    Code(gear_core::code::CodeError),
    #[error(transparent)]
    Etc(#[from] etc::Error),
}

impl From<gear_core::code::CodeError> for Error {
    fn from(err: gear_core::code::CodeError) -> Self {
        Self::Code(err)
    }
}

/// Custom result
pub type Result<T, E = Error> = std::result::Result<T, E>;
