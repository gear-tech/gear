// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Config;
use frame_support::{dispatch::DispatchInfo, traits::Contains, weights::Weight};
use scale_info::TypeInfo;
use sp_runtime::{
    codec::{Decode, DecodeWithMemTracking, Encode},
    traits::{DispatchInfoOf, Dispatchable, Implication, TransactionExtension},
    transaction_validity::{
        InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
    },
};

/// Filter `Staking::bond()` extrinsic sent from accounts that are not allowed to stake.
///
/// This will remain until all locked tokens for accounts in question are fully vested.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Default, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StakingBlackList<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config + Send + Sync> StakingBlackList<T> {
    /// Creates new transaction extension to check the call validity.
    pub fn new() -> Self {
        Self(Default::default())
    }
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for StakingBlackList<T>
where
    T::RuntimeOrigin: Clone,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, RuntimeOrigin = T::RuntimeOrigin>,
{
    const IDENTIFIER: &'static str = "StakingBlackList";
    type Implicit = ();
    type Val = ();
    type Pre = ();

    fn weight(&self, _: &T::RuntimeCall) -> Weight {
        Weight::zero()
    }

    fn validate(
        &self,
        origin: T::RuntimeOrigin,
        call: &T::RuntimeCall,
        _: &DispatchInfoOf<T::RuntimeCall>,
        _: usize,
        _: Self::Implicit,
        _: &impl Implication,
        _: TransactionSource,
    ) -> Result<(ValidTransaction, Self::Val, T::RuntimeOrigin), TransactionValidityError> {
        if T::BondCallFilter::contains(call)
            && let Ok(from) = frame_system::ensure_signed(origin.clone())
            && T::AccountFilter::contains(&from)
        {
            return Err(TransactionValidityError::Invalid(InvalidTransaction::Call));
        }
        Ok((Default::default(), (), origin))
    }

    fn prepare(
        self,
        _: Self::Val,
        _: &T::RuntimeOrigin,
        _: &T::RuntimeCall,
        _: &DispatchInfoOf<T::RuntimeCall>,
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
