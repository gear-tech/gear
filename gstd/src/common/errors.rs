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

//! Gear common errors module.
//! Enumerates errors that can occur in smart-contracts `ContractError`.
//! Errors related to conversion, decoding, message exit code, other internal
//! errors.

use core::fmt;
use gcore::msg::{ReplyError, SendError};

pub type Result<T> = core::result::Result<T, ContractError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    Convert(&'static str),
    Decode(codec::Error),
    ExitCode(i32),
    Internal(&'static str),
    Sending(SendError),
    Reply(ReplyError),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ContractError::Convert(e) => write!(f, "Conversion error: {:?}", e),
            ContractError::Decode(e) => write!(f, "Decoding codec bytes error: {}", e),
            ContractError::ExitCode(e) => write!(f, "Reply returned exit code {}", e),
            ContractError::Internal(e) => write!(f, "Internal error: {:?}", e),
            // TODO: print error when it will contain actual information
            ContractError::Sending(_) => write!(f, "Send error"),
            // TODO: print error when it will contain actual information
            ContractError::Reply(_) => write!(f, "Reply error"),
        }
    }
}

impl From<SendError> for ContractError {
    fn from(err: SendError) -> Self {
        Self::Sending(err)
    }
}

impl From<ReplyError> for ContractError {
    fn from(err: ReplyError) -> Self {
        Self::Reply(err)
    }
}
