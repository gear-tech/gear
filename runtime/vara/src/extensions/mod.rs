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

//! A list of the different weight modules for our runtime.

use crate::{AccountId, RuntimeCall};
use frame_support::{
    codec::{Decode, Encode},
    traits::Contains,
};
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{DispatchInfoOf, SignedExtension, Zero},
    transaction_validity::{InvalidTransaction, TransactionValidity, TransactionValidityError},
};

/// Disallow balances transfer
///
/// RELEASE: This is only relevant for the initial PoA run-in period and will be removed
/// from the release runtime.

struct ValueTransferCallFilter;
impl Contains<RuntimeCall> for ValueTransferCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        match call {
            RuntimeCall::Balances(_) => true,
            RuntimeCall::Gear(pallet_gear::Call::create_program { value, .. })
            | RuntimeCall::Gear(pallet_gear::Call::upload_program { value, .. })
            | RuntimeCall::Gear(pallet_gear::Call::send_message { value, .. })
            | RuntimeCall::Gear(pallet_gear::Call::send_reply { value, .. }) => !value.is_zero(),
            RuntimeCall::Utility(utility_call) => {
                match utility_call {
                    pallet_utility::Call::batch { calls }
                    | pallet_utility::Call::batch_all { calls }
                    | pallet_utility::Call::force_batch { calls } => {
                        for c in calls {
                            if Self::contains(c) {
                                return true;
                            }
                        }
                    }
                    pallet_utility::Call::as_derivative { call, .. }
                    | pallet_utility::Call::dispatch_as { call, .. }
                    | pallet_utility::Call::with_weight { call, .. } => {
                        return Self::contains(call);
                    }
                    _ => (),
                }
                false
            }
            _ => false,
        }
    }
}

#[derive(Default, Encode, Debug, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct DisableValueTransfers;

impl SignedExtension for DisableValueTransfers {
    const IDENTIFIER: &'static str = "DisableValueTransfers";
    type AccountId = AccountId;
    type Call = RuntimeCall;
    type AdditionalSigned = ();
    type Pre = ();
    fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
        Ok(())
    }
    fn validate(
        &self,
        _: &Self::AccountId,
        call: &Self::Call,
        _: &DispatchInfoOf<Self::Call>,
        _: usize,
    ) -> TransactionValidity {
        if ValueTransferCallFilter::contains(call) {
            Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
        } else {
            Ok(Default::default())
        }
    }
    fn pre_dispatch(
        self,
        _: &Self::AccountId,
        _: &Self::Call,
        _: &DispatchInfoOf<Self::Call>,
        _: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        Ok(())
    }
}
