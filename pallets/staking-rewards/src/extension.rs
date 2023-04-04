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

use crate::Config;
use frame_support::{
    codec::{Decode, Encode},
    dispatch::DispatchInfo,
    traits::Contains,
};
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{DispatchInfoOf, Dispatchable, SignedExtension},
    transaction_validity::{InvalidTransaction, TransactionValidity, TransactionValidityError},
};

/// Filter `Staking::bond()` extrinsic sent from accounts that are not allowed to stake.
///
/// This will remain until all locked tokens for accounts in question are fully vested.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Default, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StakingBlackList<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config + Send + Sync> StakingBlackList<T> {
    /// Creates new `SignedExtension` to check the call validity.
    pub fn new() -> Self {
        Self(Default::default())
    }
}

impl<T: Config + Send + Sync> SignedExtension for StakingBlackList<T>
where
    T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
    const IDENTIFIER: &'static str = "StakingBlackList";
    type AccountId = T::AccountId;
    type Call = T::RuntimeCall;
    type AdditionalSigned = ();
    type Pre = ();
    fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
        Ok(())
    }
    fn validate(
        &self,
        from: &Self::AccountId,
        call: &Self::Call,
        _: &DispatchInfoOf<Self::Call>,
        _: usize,
    ) -> TransactionValidity {
        if T::BondCallFilter::contains(call) {
            if T::AccountFilter::contains(from) {
                Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
            } else {
                Ok(Default::default())
            }
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

impl<T: Config + Send + Sync> sp_std::fmt::Debug for StakingBlackList<T> {
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        write!(f, "StakingBlackList")
    }

    #[cfg(not(feature = "std"))]
    fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        Ok(())
    }
}
