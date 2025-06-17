// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::Config;
use frame_support::{
    dispatch::{DispatchInfo, DispatchResult},
    traits::{Contains, OriginTrait},
    weights::Weight,
};
use scale_info::TypeInfo;
use sp_runtime::{
    codec::{Decode, DecodeWithMemTracking, Encode},
    traits::{
        transaction_extension::{TransactionExtension, ValidateResult},
        DispatchInfoOf, DispatchOriginOf, Dispatchable, Implication, PostDispatchInfoOf,
    },
    transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};
use sp_std::vec;

/// Filter `Staking::bond()` extrinsic sent from accounts that are not allowed to stake.
///
/// This will remain until all locked tokens for accounts in question are fully vested.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StakingBlackList<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config + Send + Sync> StakingBlackList<T> {
    pub fn new() -> Self {
        Self(Default::default())
    }
}

impl<T: Config + Send + Sync> Default for StakingBlackList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Config + Send + Sync + Send + Sync>
    TransactionExtension<<T as frame_system::Config>::RuntimeCall> for StakingBlackList<T>
where
    T::RuntimeCall: Dispatchable<
        Info = DispatchInfo,
        PostInfo = frame_support::dispatch::PostDispatchInfo,
        RuntimeOrigin = frame_system::pallet_prelude::OriginFor<T>,
    >,
    <T::RuntimeCall as Dispatchable>::RuntimeOrigin: OriginTrait<AccountId = T::AccountId>,
{
    const IDENTIFIER: &'static str = "StakingBlackList";
    type Implicit = ();
    type Val = ();
    type Pre = ();

    fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
        Ok(())
    }

    fn metadata() -> sp_std::vec::Vec<sp_runtime::traits::TransactionExtensionMetadata> {
        vec![sp_runtime::traits::TransactionExtensionMetadata {
            identifier: Self::IDENTIFIER,
            ty: scale_info::meta_type::<()>(),
            implicit: scale_info::meta_type::<Self::Implicit>(),
        }]
    }

    fn weight(&self, _call: &T::RuntimeCall) -> Weight {
        Weight::zero()
    }

    fn validate(
        &self,
        origin: DispatchOriginOf<T::RuntimeCall>,
        call: &T::RuntimeCall,
        _info: &DispatchInfoOf<T::RuntimeCall>,
        _len: usize,
        _self_implicit: Self::Implicit,
        _inherited_implication: &impl Implication,
        _source: TransactionSource,
    ) -> ValidateResult<Self::Val, T::RuntimeCall> {
        let maybe_who: Option<T::AccountId> = origin.clone().into_signer();

        if T::BondCallFilter::contains(call) {
            if let Some(ref who) = maybe_who {
                if T::AccountFilter::contains(who) {
                    return Err(TransactionValidityError::Invalid(InvalidTransaction::Call));
                }
            }
            // If `maybe_who` is `None`, it's not a signed extrinsic from a regular account,
            // so the account-based blacklist doesn't apply.
        }
        Ok((Default::default(), (), origin))
    }

    fn prepare(
        self,
        _val: Self::Val,
        _origin: &<T::RuntimeCall as Dispatchable>::RuntimeOrigin,
        _call: &T::RuntimeCall,
        _info: &DispatchInfoOf<T::RuntimeCall>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        Ok(())
    }

    fn post_dispatch(
        _pre: Self::Pre, // Changed from Option<Self::Pre>
        _info: &DispatchInfoOf<T::RuntimeCall>,
        _post_info: &mut PostDispatchInfoOf<T::RuntimeCall>, // Added mut
        _len: usize,
        _result: &DispatchResult,
    ) -> Result<(), TransactionValidityError> {
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
