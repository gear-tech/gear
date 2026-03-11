// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use ethexe_common::{Announce, Digest, HashOf, consensus::BatchCommitmentValidationRequest};
use gprimitives::CodeId;

/// This struct represents the limits for batch.
#[derive(Debug, Clone)]
pub struct BatchLimits {
    // MOVED from ValidatorCore.
    /// Minimum deepness threshold to create chain commitment even if there are no transitions.
    pub chain_deepness_threshold: u32,
    /// Time limit in blocks for announce to be committed after its creation.
    pub commitment_delay_limit: u32,
    /// The maximum number of codes batch commitment can contain.
    pub max_codes_limit: usize,
}

#[derive(Debug, derive_more::Display, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    #[display("accepted batch commitment with digest {_0:?}")]
    Accepted(Digest),
    #[display("rejected batch commitment request {request:?} : {reason}")]
    Rejected {
        request: BatchCommitmentValidationRequest,
        reason: ValidationRejectReason,
    },
}

#[derive(Debug, derive_more::Display, Clone, PartialEq, Eq)]
pub enum ValidationRejectReason {
    #[display("batch commitment is empty")]
    EmptyBatch,
    #[display("batch commitment request contains duplicate code ids")]
    CodesHasDuplicates,
    #[display("code id {_0} is not waiting for commitment")]
    CodeNotWaitingForCommitment(CodeId),
    #[display("code id {_0} is not processed yet")]
    CodeIsNotProcessedYet(CodeId),
    #[display("requested head announce {requested} is not the best announce {best}")]
    HeadAnnounceIsNotBest {
        requested: HashOf<Announce>,
        best: HashOf<Announce>,
    },
    #[display("requested head announce {_0} is not computed by this node")]
    HeadAnnounceNotComputed(HashOf<Announce>),
    #[display(
        "received batch contains validators commitment, but it's not time for validators election yet"
    )]
    ValidatorsNotReady,
    #[display(
        "received batch contains rewards commitment, but it's not time for rewards distribution yet"
    )]
    RewardsNotReady,
    #[display("batch commitment digest mismatch: expected {expected}, found {found}")]
    BatchDigestMismatch { expected: Digest, found: Digest },
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
#[display("Code not found: {_0}")]
pub struct CodeNotValidatedError(pub CodeId);
