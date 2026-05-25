// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::abi::{IRouter, utils::*};
use ethexe_common::{Digest, events::router::*};

impl From<IRouter::BatchCommitted> for BatchCommittedEvent {
    fn from(value: IRouter::BatchCommitted) -> Self {
        Self {
            digest: Digest(bytes32_to_h256(value.hash).0),
        }
    }
}

impl From<IRouter::MBCommitted> for MBCommittedEvent {
    fn from(value: IRouter::MBCommitted) -> Self {
        Self(bytes32_to_h256(value.head))
    }
}

impl From<IRouter::EBCommitted> for EBCommittedEvent {
    fn from(value: IRouter::EBCommitted) -> Self {
        Self(bytes32_to_h256(value.ethBlockHash))
    }
}

impl From<IRouter::CodeGotValidated> for CodeGotValidatedEvent {
    fn from(value: IRouter::CodeGotValidated) -> Self {
        Self {
            code_id: bytes32_to_code_id(value.codeId),
            valid: value.valid,
        }
    }
}

impl From<IRouter::ComputationSettingsChanged> for ComputationSettingsChangedEvent {
    fn from(value: IRouter::ComputationSettingsChanged) -> Self {
        Self {
            threshold: value.threshold,
            wvara_per_second: value.wvaraPerSecond,
        }
    }
}

impl From<IRouter::ProgramCreated> for ProgramCreatedEvent {
    fn from(value: IRouter::ProgramCreated) -> Self {
        Self {
            actor_id: address_to_actor_id(value.actorId),
            code_id: bytes32_to_code_id(value.codeId),
        }
    }
}

impl From<IRouter::StorageSlotChanged> for StorageSlotChangedEvent {
    fn from(value: IRouter::StorageSlotChanged) -> Self {
        Self {
            slot: bytes32_to_h256(value.slot),
        }
    }
}

impl From<IRouter::ValidatorsCommittedForEra> for ValidatorsCommittedForEraEvent {
    fn from(value: IRouter::ValidatorsCommittedForEra) -> Self {
        Self {
            era_index: value
                .eraIndex
                .try_into()
                .expect("next era start timestamp is too large"),
        }
    }
}
