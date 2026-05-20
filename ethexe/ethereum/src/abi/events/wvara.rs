// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::abi::{IWrappedVara, utils::*};
use ethexe_common::events::wvara::*;
use gprimitives::U256;

impl From<IWrappedVara::Transfer> for TransferEvent {
    fn from(value: IWrappedVara::Transfer) -> Self {
        Self {
            from: address_to_actor_id(value.from),
            to: address_to_actor_id(value.to),
            value: uint256_to_u128_lossy(value.value),
        }
    }
}

impl From<IWrappedVara::Approval> for ApprovalEvent {
    fn from(value: IWrappedVara::Approval) -> Self {
        Self {
            owner: address_to_actor_id(value.owner),
            spender: address_to_actor_id(value.spender),
            value: U256(value.value.into_limbs()),
        }
    }
}
