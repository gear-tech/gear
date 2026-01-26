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

use thiserror::Error;

/// Canonical DKG error categories used across the engine.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum DkgErrorKind {
    #[error("DKG already in progress")]
    AlreadyInProgress,
    #[error("No active protocol")]
    NoActiveProtocol,
    #[error("Missing DKG config")]
    MissingConfig,
    #[error("Session ID mismatch")]
    SessionIdMismatch,
    #[error("Unknown participant")]
    UnknownParticipant,
    #[error("Unknown complainer")]
    UnknownComplainer,
    #[error("Unknown offender")]
    UnknownOffender,
    #[error("Self not in participants list")]
    SelfNotInParticipants,
    #[error("Duplicate participants detected")]
    DuplicateParticipants,
    #[error("Invalid participant identifier")]
    InvalidParticipantIdentifier,
    #[error("Round1 not complete")]
    Round1NotComplete,
    #[error("Round2 not complete")]
    Round2NotComplete,
    #[error("Missing round2 packages for self")]
    MissingRound2PackagesForSelf,
    #[error("Self not in validators list")]
    SelfNotInValidatorsList,
    #[error("Validator index out of range")]
    ValidatorIndexOutOfRange,
}

/// Extension for downcasting `anyhow::Error` into `DkgErrorKind`.
pub trait DkgErrorExt {
    fn dkg_error_kind(&self) -> Option<DkgErrorKind>;
}

impl DkgErrorExt for anyhow::Error {
    fn dkg_error_kind(&self) -> Option<DkgErrorKind> {
        self.downcast_ref::<DkgErrorKind>().copied()
    }
}
